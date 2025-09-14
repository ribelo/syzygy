//! Example demonstrating Phase 2: BlockingScheduler and InlineAsync
//! This shows how to run async effects without spawning threads

use syzygy::executor::{InlineAsync, Task};
use syzygy::prelude::*;
use syzygy::scheduler::BlockingScheduler;

#[derive(Debug, Clone)]
enum Event {
    StartWork,
    WorkCompleted(String),
    StartSyncWork,
    SyncWorkCompleted(String),
}

#[derive(Debug, Clone)]
enum Effect {
    DoAsyncWork,
    DoSyncWork,
}

#[derive(Debug, Default)]
struct Model {
    work_count: usize,
}

fn update(event: Event, ctx: &mut EventContext<Event, Effect, Model>) -> Command<Event, Effect> {
    let model = ctx.model_mut();
    match event {
        Event::StartWork => {
            println!("Starting work...");
            Command::effect(Effect::DoAsyncWork)
        }
        Event::WorkCompleted(msg) => {
            model.work_count += 1;
            println!("Work completed: {msg}");
            if model.work_count == 1 {
                Command::event(Event::StartSyncWork)
            } else {
                Command::none()
            }
        }
        Event::StartSyncWork => Command::effect(Effect::DoSyncWork),
        Event::SyncWorkCompleted(msg) => {
            model.work_count += 1;
            println!("Work completed: {msg}");
            Command::none()
        }
    }
}

fn handle_effects(effect: Effect, ctx: &EffectContext<Event, ()>) -> Task<Event, ()> {
    match effect {
        Effect::DoAsyncWork => {
            // Using ctx.spawn for async work
            ctx.spawn::<InlineAsync<Event>, _, _>(|_ctx| async move {
                // Simulate async work (would be actual I/O in real code)
                Event::WorkCompleted("Async work done".to_string()).into()
            })

            // Alternative: Direct Task creation
            // Task::async_task::<InlineAsync<Event>, _>(async move {
            //     Event::WorkCompleted("Async work done".to_string()).into()
            // })
        }
        Effect::DoSyncWork => {
            // Using ctx.spawn for work
            ctx.spawn::<InlineAsync<Event>, _, _>(|_ctx| async move {
                Event::SyncWorkCompleted("Blocking work done".to_string()).into()
            })

            // Alternative: Direct Task creation
            // Task::async_task::<InlineAsync<Event>, _>(async move {
            //     Event::SyncWorkCompleted("Blocking work done".to_string()).into()
            // })
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
        .with_async_executor(InlineAsync::<Event>::new())
        .build();

    let mut runner = Runner::new(core, shell);

    // Send initial event
    runner.core_mut().send_event(Event::StartWork);

    // Use BlockingScheduler - runs futures to completion inline
    let scheduler = BlockingScheduler;

    // Process all events and effects
    while runner.step_with(scheduler)? {
        // Everything runs sequentially with BlockingScheduler
    }

    println!("\n✅ Demo complete!");
    println!("All work was executed without spawning threads.");
    println!("BlockingScheduler + InlineAsync = true single-threaded execution.");

    Ok(())
}
