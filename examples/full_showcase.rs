use futures::stream;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppState {
    counter: i32,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    Increment(i32),
    Tick,
    SetStatus(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    Save(i32),
    Watch,
}

#[derive(Debug, Clone)]
struct Prefix(String);

fn increment(amount: i32, mut counter: Counter) -> Command<AppEvent, AppEffect> {
    *counter += amount;
    Command::effect(AppEffect::Save(*counter))
}

fn tick(_: (), mut counter: Counter) -> Command<AppEvent, AppEffect> {
    *counter += 1;
    Command::none()
}

fn set_status(new_status: String, mut status: Status) -> Command<AppEvent, AppEffect> {
    *status = new_status;
    Command::none()
}

fn dispatch_event(event: AppEvent, ctx: &EventContext<AppState>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Increment(amount) => increment.handle(amount, ctx),
        AppEvent::Tick => tick.handle((), ctx),
        AppEvent::SetStatus(status) => set_status.handle(status, ctx),
    }
}

fn save_effect(value: i32, prefix: Res<Prefix>) -> Task<AppEvent, AppEffect> {
    let message = format!("{} saved={value}", prefix.0);
    Task::once(async move { Command::event(AppEvent::SetStatus(message)) })
}

fn watch_effect(_: (), _prefix: Res<Prefix>) -> Task<AppEvent, AppEffect> {
    Task::stream(stream::iter([
        Command::event(AppEvent::Tick),
        Command::event(AppEvent::Tick),
    ]))
}

fn dispatch_effect(effect: AppEffect, ctx: &EffectContext) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::Save(value) => save_effect.handle(value, ctx),
        AppEffect::Watch => watch_effect.handle((), ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppState::default())
        .with_resource(Prefix("showcase".to_string()))
        .event_handler(dispatch_event)
        .effect_handler(dispatch_effect)
        .build();

    runner.core().try_send_event(AppEvent::Increment(2))?;
    runner.step()?;
    runner.step()?;

    runner
        .core()
        .try_send_event(AppEvent::SetStatus("watch".to_string()))?;
    runner.step()?;

    runner
        .shell_mut()
        .dispatch_command(Command::effect(AppEffect::Watch))?;
    runner.step()?;
    runner.step()?;

    println!(
        "counter={} status={}",
        runner.model().counter,
        runner.model().status
    );
    Ok(())
}
