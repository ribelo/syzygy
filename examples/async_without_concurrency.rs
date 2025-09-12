//! Example demonstrating "asynchrony without concurrency"
//! 
//! This shows how Syzygy can run async effects without requiring
//! concurrent execution, matching the principles from Loris Cro's article.

use syzygy::prelude::*;
use syzygy::executor::{InlineAsync, Task};
use syzygy::scheduler::BlockingScheduler;

#[derive(Debug, Clone)]
enum Event {
    SaveFiles,
    FilesSaved { count: usize },
}

#[derive(Debug, Clone)]
enum Effect {
    SaveFile { name: String },
}

#[derive(Debug, Default)]
struct Model {
    files_saved: usize,
}

fn update(event: Event, ctx: &mut EventContext<Event, Effect, Model>) -> Command<Event, Effect> {
    let model = ctx.model_mut();
    match event {
        Event::SaveFiles => {
            // These effects express asynchrony (can complete in any order)
            // but don't require concurrency (can run sequentially)
            Command::batch([
                Command::effect(Effect::SaveFile { name: "file_a.txt".into() }),
                Command::effect(Effect::SaveFile { name: "file_b.txt".into() }),
            ])
        }
        Event::FilesSaved { count } => {
            model.files_saved += count;
            println!("✅ {} files saved (total: {})", count, model.files_saved);
            Command::none()
        }
    }
}

fn handle_effects(effect: Effect, ctx: &EffectContext<Event, ()>) -> Task<Event, ()> {
    match effect {
        Effect::SaveFile { name } => {
            // Using ctx.spawn_best_effort - provides access to context for resources, events, etc.
            ctx.spawn_best_effort::<InlineAsync<Event>, _, _>(move |_ctx| async move {
                println!("📝 Saving {}...", name);
                // In real code, you could use ctx here to:
                // - Access resources: ctx.resources()
                // - Send events: ctx.send_event()
                // - Spawn more tasks: ctx.spawn_best_effort()
                println!("✅ Saved {}", name);
                vec![Event::FilesSaved { count: 1 }].into()  // Vec<E> converts to Outcome<E>
            })
            
            // Alternative: Direct Task creation (when you don't need context)
            // Task::best_effort::<InlineAsync<Event>, _>(async move {
            //     println!("📝 Saving {}...", name);
            //     println!("✅ Saved {}", name);
            //     vec![Event::FilesSaved { count: 1 }].into()
            // })
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Asynchrony Without Concurrency Demo ===\n");
    
    // Build the system with InlineAsync executor
    let (core, shell) = Syzygy::builder()
        .model(Model::default())
        .event_handler(update)
        .effect_handler(handle_effects)
        .with_async_executor(InlineAsync::<Event>::new())
        .build();
    
    let mut runner = Runner::new(core, shell);
    
    // Use BlockingScheduler - runs futures to completion inline
    let scheduler = BlockingScheduler;
    
    println!("Sending SaveFiles event...\n");
    let _ = runner.core_mut().send_event(Event::SaveFiles);
    
    // Process with blocking scheduler - no concurrency, just asynchrony
    while runner.step_with(scheduler)? {
        // Effects run sequentially, not concurrently
        // But they can still complete in any order (asynchrony)
    }
    
    println!("\n🎉 Demo complete!");
    println!("Files were saved asynchronously (order didn't matter)");
    println!("But without concurrency (ran sequentially)");
    
    Ok(())
}
