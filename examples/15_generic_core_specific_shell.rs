use syzygy::prelude::*;

// Reusable feature core: pure model + events + effects.
#[derive(Debug, Default, Model)]
struct CounterFeatureModel {
    #[model(wrapper = Count)]
    count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CounterFeatureEvent {
    Increment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CounterFeatureEffect {
    PersistCount(i32),
}

fn counter_feature_increment(
    feature: &mut CounterFeatureModel,
) -> Command<CounterFeatureEvent, CounterFeatureEffect> {
    feature.count += 1;
    Command::effect(CounterFeatureEffect::PersistCount(feature.count))
}

// Specific shell/application composition.
#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(part)]
    feature: CounterFeatureModel,
    #[model(wrapper = LastPersisted)]
    last_persisted: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    UserPressedIncrement,
    Feature(CounterFeatureEvent),
    Persisted(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    Feature(CounterFeatureEffect),
}

fn app_event_handler(
    event: AppEvent,
    ctx: &EventContext<AppModel>,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserPressedIncrement => {
            Command::event(AppEvent::Feature(CounterFeatureEvent::Increment))
        }
        AppEvent::Feature(feature_event) => {
            let child_command = match feature_event {
                CounterFeatureEvent::Increment => handle!(counter_feature_increment, ctx),
            };

            child_command
                .map_event(AppEvent::Feature)
                .map_effect(AppEffect::Feature)
        }
        AppEvent::Persisted(value) => {
            *LastPersisted::extract_mut(ctx).get_mut() = Some(value);
            Command::none()
        }
    }
}

fn app_effect_handler(effect: AppEffect, _ctx: &EffectContext<'_>) -> Task<AppEvent, AppEffect> {
    match effect {
        // Specific shell behavior: this app decides how "persist" is interpreted.
        AppEffect::Feature(CounterFeatureEffect::PersistCount(value)) => {
            println!("persisting count in app shell: {value}");
            Task::resolved(Command::event(AppEvent::Persisted(value)))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(app_event_handler)
        .effect_handler(app_effect_handler)
        .build()?;

    app.core().emit(AppEvent::UserPressedIncrement);
    app.step()?;
    app.step()?;
    app.step()?;

    println!("feature count = {}", app.model().feature.count);
    println!("last persisted = {:?}", app.model().last_persisted);
    Ok(())
}
