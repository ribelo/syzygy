use syzygy::prelude::*;

// ==========================================
// 1. The Child Component (Counter)
// ==========================================

/// ## Why separate the model?
/// A core principle of The Elm Architecture is composability.
/// By creating a standalone `CounterModel`, `CounterEvent`, and `CounterEffect`,
/// this component becomes fully decoupled from any parent application.
#[derive(Debug, Default, Model)]
pub struct CounterModel {
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterEvent {
    Increment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterEffect {
    PlaySound,
}

/// The child operates purely on its own model.
fn increment(_: (), counter: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
    counter.count += 1;
    Command::effect(CounterEffect::PlaySound)
}

// ==========================================
// 2. The Parent Component (App)
// ==========================================

#[derive(Debug, Default, Model)]
struct AppModel {
    /// ## Why `#[model(part)]`?
    ///
    /// The parent model composes child models. Using `#[model(part)]` makes
    /// `CounterModel` itself directly extractable from `AppModel`.
    #[model(part)]
    counter: CounterModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    ChildMsg(CounterEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    ChildFx(CounterEffect),
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::ChildMsg(child_event) => {
            // ## Event delegation pattern
            // Since `#[model(part)]` makes `CounterModel` directly extractable,
            // we can run child handlers using the parent context.
            let child_cmd = match child_event {
                CounterEvent::Increment => handle!(increment, ctx),
            };

            // Map the child's Command types back to the parent's generic types
            child_cmd
                .map_event(AppEvent::ChildMsg)
                .map_effect(AppEffect::ChildFx)
        }
    }
}

fn handle_effect(effect: AppEffect, _ctx: &EffectContext<'_>) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::ChildFx(CounterEffect::PlaySound) => {
            println!("*Beep!*");
            Task::none()
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()?;

    app.core()
        .try_send(AppEvent::ChildMsg(CounterEvent::Increment))?;

    // Process the event
    app.step()?;
    // Process the resulting effect
    app.step()?;

    println!("Counter value: {}", app.model().counter.count);
    Ok(())
}
