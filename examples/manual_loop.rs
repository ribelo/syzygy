//! Drives `Core` and `Shell` manually without the runner abstraction.
//!
//! Build with
//! ```bash
//! cargo run --example manual_loop --features examples
//! ```

use syzygy::executor::{ExecutorRegistry, InlineAsync, Outcome, Task};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct AppModel {
    logs: Vec<String>,
    completed: bool,
}

#[derive(Debug, Clone)]
enum AppEvent {
    Start,
    Completed(String),
}

#[derive(Debug, Clone)]
enum AppEffect {
    ProduceMessage,
}

fn event_handler(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppModel>,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Start => on_start(),
        AppEvent::Completed(message) => on_completed(ctx.model_mut(), message),
    }
}

fn on_start() -> Command<AppEvent, AppEffect> {
    Command::effect(AppEffect::ProduceMessage)
}

fn on_completed(model: &mut AppModel, message: String) -> Command<AppEvent, AppEffect> {
    model.logs.push(message);
    model.completed = true;
    Command::none()
}

fn effect_handler(effect: AppEffect, _ctx: EffectContext<AppEvent, ()>) -> Task<AppEvent, ()> {
    match effect {
        AppEffect::ProduceMessage => produce_message(),
    }
}

fn produce_message() -> Task<AppEvent, ()> {
    Task::async_task::<InlineAsync<AppEvent>, _>(async move {
        Outcome::Event(AppEvent::Completed("effect finished".to_string()))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = ExecutorRegistry::new();
    registry.insert_async(InlineAsync::<AppEvent>::new());

    let (mut core, mut shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .with_executor_registry(registry)
        .build()
        .split();

    core.send_event(AppEvent::Start);

    let scheduler = syzygy::scheduler::scheduler();
    while syzygy::syzygy::step_core_shell(&mut core, &mut shell, scheduler.clone())? {}

    println!("Logs: {:?}", core.model().logs);
    Ok(())
}
