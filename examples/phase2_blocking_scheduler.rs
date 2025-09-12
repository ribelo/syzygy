//! Phase 2: BlockingScheduler and InlineAsync Executor Example
//!
//! This example demonstrates the new BlockingScheduler and InlineAsync executor
//! introduced in Phase 2 of Syzygy development.

use syzygy::prelude::*;
use syzygy::executor::{Outcome, Task};
use std::time::Duration;

// Define our events and effects
#[derive(Debug, Clone)]
enum Event {
    Start,
    WorkComplete(String),
    Error(String),
}

#[derive(Debug, Clone)]
enum Effect {
    DoAsyncWork,
    DoBlockingWork,
}

// Simple model
#[derive(Debug, Default)]
struct Model {
    results: Vec<String>,
}

// Update function
fn update(event: Event, ctx: &mut EventContext<Event, Effect, Model>) -> Command<Event, Effect> {
    let model = ctx.model_mut();

    match event {
        Event::Start => {
            println!("Starting work...");
            Command::effect(Effect::DoAsyncWork)
        }
        Event::WorkComplete(result) => {
            println!("Work completed: {}", result);
            model.results.push(result);
            Command::effect(Effect::DoBlockingWork)
        }
        Event::Error(msg) => {
            println!("Error: {}", msg);
            Command::none()
        }
    }
}

// Effect handler demonstrating different spawning strategies
fn handle_effects(
    effect: Effect,
    _ctx: &EffectContext<Event, ()>,
) -> Task<Event, ()> {
    match effect {
        Effect::DoAsyncWork => {
            Task::best_effort::<syzygy::executor::InlineAsync<Event>, _>(async move {
                std::thread::sleep(Duration::from_millis(100));
                Outcome::Event(Event::WorkComplete("Async work done".to_string()))
            })
        }
        Effect::DoBlockingWork => {
            Task::best_effort::<syzygy::executor::InlineAsync<Event>, _>(async move {
                // Simulate some blocking work
                std::thread::sleep(Duration::from_millis(50));
                Outcome::Event(Event::WorkComplete("Blocking work done".to_string()))
            })
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Phase 2: BlockingScheduler and InlineAsync Demo ===\n");

    // Build the system with InlineAsync executor
    let (core, shell) = Syzygy::builder()
        .model(Model::default())
        .event_handler(update)
        .effect_handler(handle_effects)
        .with_async_executor(syzygy::executor::InlineAsync::new())
        .build();

    // Create a runner with BlockingScheduler
    let mut runner = Runner::new(core, shell);

    // Send initial event
    runner.core().send_event(Event::Start);

    // Use BlockingScheduler for synchronous execution
    runner.tick(syzygy::scheduler::blocking_scheduler())?;

    // Check results
    let model = runner.core().model();
    println!("\nFinal results: {:?}", model.results);
    println!("Demo completed successfully!");

    Ok(())
}