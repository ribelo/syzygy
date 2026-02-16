//! Drives `Core` and `Shell` manually without the runner abstraction.
//!
//! Build with
//! ```bash
//! cargo run --example manual_loop --features examples
//! ```

use std::time::Duration;

use syzygy::executor::{InlineAsync, Task};
use syzygy::prelude::*;
use syzygy::syzygy::SyzygyConfig;

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
    cmd::effect(AppEffect::ProduceMessage)
}

fn on_completed(model: &mut AppModel, message: String) -> Command<AppEvent, AppEffect> {
    model.logs.push(message);
    model.completed = true;
    cmd::none()
}

fn effect_handler(effect: AppEffect, _resources: ()) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::ProduceMessage => produce_message(),
    }
}

fn produce_message() -> Task<AppEvent, AppEffect> {
    Task::async_on::<InlineAsync, _>(async move {
        cmd::event(AppEvent::Completed("effect finished".to_string()))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut core, mut shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .with_effect_channel_capacity(Some(256))
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
        .with_async_executor(InlineAsync::new())
        .build()
        .split();

    core.try_send_event(AppEvent::Start)
        .expect("event channel should be open");

    while syzygy::syzygy::step_core_shell(&mut core, &mut shell)? {}

    println!("Logs: {:?}", core.model().logs);
    Ok(())
}
