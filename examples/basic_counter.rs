//! Minimal counter demonstrating the core Syzygy loop.
//!
//! Build with
//! ```bash
//! cargo run --example basic_counter --features examples
//! ```

use syzygy::executor::{ExecutorRegistry, Task};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct CounterModel {
    value: i32,
}

#[derive(Debug, Clone)]
enum CounterEvent {
    Increment,
    Decrement,
}

#[derive(Debug, Clone)]
enum CounterEffect {
    Log(String),
}

fn update_counter(
    event: CounterEvent,
    ctx: &mut EventContext<CounterEvent, CounterEffect, CounterModel>,
) -> Command<CounterEvent, CounterEffect> {
    let model = ctx.model_mut();
    match event {
        CounterEvent::Increment => {
            model.value += 1;
            Command::effect(CounterEffect::Log(format!(
                "Count incremented to {}",
                model.value
            )))
        }
        CounterEvent::Decrement => {
            model.value -= 1;
            Command::effect(CounterEffect::Log(format!(
                "Count decremented to {}",
                model.value
            )))
        }
    }
}

fn handle_effect(
    effect: CounterEffect,
    _ctx: &EffectContext<CounterEvent, ()>,
) -> Task<CounterEvent, ()> {
    let CounterEffect::Log(message) = effect;
    println!("{message}");
    Task::none()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ExecutorRegistry::new();

    let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
        .model(CounterModel::default())
        .event_handler(update_counter)
        .effect_handler(handle_effect)
        .with_executor_registry(registry)
        .build_runner();

    runner.core().send_event(CounterEvent::Increment);
    runner.core().send_event(CounterEvent::Increment);
    runner.core().send_event(CounterEvent::Decrement);

    let scheduler = syzygy::scheduler::scheduler();
    while runner.step_with(scheduler.clone())? {}

    println!("Final count: {}", runner.core().model().value);
    Ok(())
}
