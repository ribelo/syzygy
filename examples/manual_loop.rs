//! Drives `Core` and `Shell` manually without the runner abstraction.
//!
//! Build with
//! ```bash
//! cargo run --example manual_loop --features examples
//! ```

use syzygy::executor::{ExecutorRegistry, Task};
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

fn update(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppModel>,
) -> Command<AppEvent, AppEffect> {
    let model = ctx.model_mut();
    match event {
        AppEvent::Start => Command::effect(AppEffect::ProduceMessage),
        AppEvent::Completed(message) => {
            model.logs.push(message);
            model.completed = true;
            Command::none()
        }
    }
}

fn handle_effect(effect: AppEffect, _ctx: &EffectContext<AppEvent, ()>) -> Task<AppEvent, ()> {
    match effect {
        AppEffect::ProduceMessage => {
            Task::events(vec![AppEvent::Completed("effect finished".to_string())])
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ExecutorRegistry::new();

    let (mut core, mut shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(update)
        .effect_handler(handle_effect)
        .with_executor_registry(registry)
        .build()
        .split();

    core.send_event(AppEvent::Start);

    let scheduler = syzygy::scheduler::scheduler();
    while syzygy::runner::step_core_shell(&mut core, &mut shell, scheduler.clone())? {}

    println!("Logs: {:?}", core.model().logs);
    Ok(())
}
