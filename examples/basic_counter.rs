//! Minimal counter demonstrating the core Syzygy loop.
//!
//! Build with
//! ```bash
//! cargo run --example basic_counter --features examples
//! ```

use syzygy::executor::{InlineAsync, Outcome, Task};
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

fn event_handler(
    event: CounterEvent,
    ctx: &mut EventContext<CounterEvent, CounterEffect, CounterModel>,
) -> Command<CounterEvent, CounterEffect> {
    match event {
        CounterEvent::Increment => on_increment(ctx.model_mut()),
        CounterEvent::Decrement => on_decrement(ctx.model_mut()),
    }
}

fn on_increment(model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
    model.value += 1;
    Command::effect(CounterEffect::Log(format!(
        "Count incremented to {}",
        model.value
    )))
}

fn on_decrement(model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
    model.value -= 1;
    Command::effect(CounterEffect::Log(format!(
        "Count decremented to {}",
        model.value
    )))
}

fn effect_handler(effect: CounterEffect, _ctx: EffectContext<CounterEvent>) -> Task<CounterEvent> {
    match effect {
        CounterEffect::Log(message) => log_message(message),
    }
}

fn log_message(message: String) -> Task<CounterEvent> {
    Task::async_owned::<InlineAsync<CounterEvent>, _, _>(|_ctx, _resources| async move {
        println!("{message}");
        Outcome::None
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
        .model(CounterModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .with_async_executor(InlineAsync::<CounterEvent>::new())
        .build();

    runner.core().send_event(CounterEvent::Increment);
    runner.core().send_event(CounterEvent::Increment);
    runner.core().send_event(CounterEvent::Decrement);

    while runner.step()? {}

    println!("Final count: {}", runner.core().model().value);
    Ok(())
}
