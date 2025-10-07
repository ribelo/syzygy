//! Drives `Core` and `Shell` manually without the runner abstraction.
//!
//! Build with
//! ```bash
//! cargo run --example manual_loop --features examples
//! ```

use syzygy::executor::{InlineAsync, Task};
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

fn event_handler(event: AppEvent, model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Start => on_start(),
        AppEvent::Completed(message) => on_completed(model, message),
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

fn effect_handler(effect: AppEffect, _resources: ()) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::ProduceMessage => produce_message(),
    }
}

fn produce_message() -> Task<AppEvent, AppEffect> {
    Task::async_on::<InlineAsync<AppEvent>, _>(async move {
        Command::event(AppEvent::Completed("effect finished".to_string()))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut core, mut shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .with_async_executor(InlineAsync::<AppEvent>::new())
        .build()
        .split();

    core.send_event(AppEvent::Start);

    while syzygy::syzygy::step_core_shell(&mut core, &mut shell)? {}

    println!("Logs: {:?}", core.model().logs);
    Ok(())
}
