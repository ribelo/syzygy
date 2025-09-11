//! Example 01: Basic TEA Pattern with Magic Handlers
//!
//! This example demonstrates the fundamental TEA (The Elm Architecture) pattern in Syzygy
//! using magic handlers for clean, focused event processing.
//! You'll learn:
//! - Basic app structure with model, events, and effects
//! - Magic handlers with automatic parameter extraction
//! - Event processing and command creation
//! - Core/Shell orchestration with Runner

use futures::FutureExt;
use syzygy::executor::{Task, TokioIo};
use syzygy::prelude::*;

// ============================================================================
// Step 1: Define your application state (Model)
// ============================================================================

#[derive(Debug, Default)]
struct CounterModel {
    count: i32,
    message: String,
}

#[derive(Debug, Clone, Default)]
struct AppConfig {
    max_count: i32,
    enable_sound: bool,
}

// ============================================================================
// Step 2: Define events that can happen in your app
// ============================================================================

#[derive(Debug, Clone)]
enum CounterEvent {
    Increment,
    Decrement,
    Reset,
    SetMessage(String),
    CheckLimit,
}

// ============================================================================
// Step 3: Define side effects your app can trigger
// ============================================================================

#[derive(Debug, Clone)]
enum CounterEffect {
    LogMessage(String),
    PlaySound,
    SaveCount(i32),
}

// ============================================================================
// Step 4: Magic Event Handlers - Clean and Focused
// ============================================================================

/// Handle increment with automatic model extraction
fn handle_increment(
    event: CounterEvent,
    counter: &mut CounterModel,
) -> Command<CounterEvent, CounterEffect> {
    if let CounterEvent::Increment = event {
        counter.count += 1;
        counter.message = format!("Count incremented to {}", counter.count);

        Command::batch([
            Command::effect(CounterEffect::LogMessage(counter.message.clone())),
            Command::effect(CounterEffect::SaveCount(counter.count)),
            Command::effect(CounterEffect::PlaySound), // Config will be checked in effect handler
            Command::event(CounterEvent::CheckLimit),  // Always check limit
        ])
    } else {
        Command::none()
    }
}

/// Handle decrement with automatic model extraction
fn handle_decrement(
    event: CounterEvent,
    counter: &mut CounterModel,
) -> Command<CounterEvent, CounterEffect> {
    if let CounterEvent::Decrement = event {
        counter.count -= 1;
        counter.message = format!("Count decremented to {}", counter.count);

        Command::batch([
            Command::effect(CounterEffect::LogMessage(counter.message.clone())),
            Command::effect(CounterEffect::SaveCount(counter.count)),
        ])
    } else {
        Command::none()
    }
}

/// Handle reset - simple model mutation
fn handle_reset(
    event: CounterEvent,
    counter: &mut CounterModel,
) -> Command<CounterEvent, CounterEffect> {
    if let CounterEvent::Reset = event {
        counter.count = 0;
        counter.message = "Counter reset".to_string();

        Command::batch([
            Command::effect(CounterEffect::LogMessage("Counter was reset".to_string())),
            Command::effect(CounterEffect::SaveCount(0)),
            Command::effect(CounterEffect::PlaySound), // Config checked in effect handler
        ])
    } else {
        Command::none()
    }
}

/// Handle message setting - simple event-only handler
fn handle_set_message(
    event: CounterEvent,
    counter: &mut CounterModel,
) -> Command<CounterEvent, CounterEffect> {
    if let CounterEvent::SetMessage(message) = event {
        counter.message = message;
        Command::effect(CounterEffect::LogMessage(format!(
            "Message set to: {}",
            counter.message
        )))
    } else {
        Command::none()
    }
}

/// Handle limit checking - read-only access to model
fn handle_check_limit(
    event: CounterEvent,
    counter: &CounterModel,
) -> Command<CounterEvent, CounterEffect> {
    if let CounterEvent::CheckLimit = event {
        // Config max_count will be checked in effect handler
        Command::effect(CounterEffect::LogMessage(format!(
            "Checking limit for current count: {}",
            counter.count
        )))
    } else {
        Command::none()
    }
}

// ============================================================================
// Main Update Function - Dispatches to Magic Handlers
// ============================================================================

fn update_counter(
    event: CounterEvent,
    ctx: &mut EventContext<CounterEvent, CounterEffect, CounterModel>,
) -> Command<CounterEvent, CounterEffect> {
    // Dispatch to appropriate magic handler based on event type
    match event {
        CounterEvent::Increment => handle_increment(event, ctx.model_mut()),
        CounterEvent::Decrement => handle_decrement(event, ctx.model_mut()),
        CounterEvent::Reset => handle_reset(event, ctx.model_mut()),
        CounterEvent::SetMessage(_) => handle_set_message(event, ctx.model_mut()),
        CounterEvent::CheckLimit => handle_check_limit(event, ctx.model()),
    }
}

// ============================================================================
// Step 5: Magic Effect Handlers
// ============================================================================

/// Handle logging effects - simple effect-only magic handler
fn handle_log_effect(effect: CounterEffect) {
    if let CounterEffect::LogMessage(message) = effect {
        println!("LOG: {message}");
    }
}

/// Handle sound effects with config access - resource extraction magic handler
fn handle_sound_effect(effect: CounterEffect, config: &AppConfig) {
    if let CounterEffect::PlaySound = effect {
        if config.enable_sound {
            println!("BEEP! (sound effect)");
        } else {
            println!("(sound disabled)");
        }
    }
}

/// Handle save effects - now returns `Task`
fn handle_save_effect(effect: CounterEffect) -> Task<CounterEvent, AppConfig> {
    if let CounterEffect::SaveCount(count) = effect {
        println!("SAVE: Counter value {count} saved to storage");
        // Could send a completion event if needed
        Task::future_on::<TokioIo, _, _, _>(move |_ctx| {
            async move {
                // Simulate async save operation
                #[cfg(feature = "tokio")]
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                // Return completion event
                CounterEvent::SetMessage("Saved!".to_string())
            }
            .boxed()
        })
    } else {
        Task::events(vec![])
    }
}

/// Handle limit checking in effects - now returns `Task`
fn handle_limit_check_effect(
    effect: CounterEffect,
    config: &AppConfig,
) -> Task<CounterEvent, AppConfig> {
    if let CounterEffect::LogMessage(message) = effect {
        if message.contains("Checking limit") {
            // Extract count from message or use context
            if let Some(count_str) = message.split(": ").nth(1)
                && let Ok(count) = count_str.parse::<i32>()
            {
                if count >= config.max_count {
                    println!("LOG: {message}");
                    println!(
                        "WARNING: Counter reached maximum value of {}!",
                        config.max_count
                    );
                    // Return warning event
                    return Task::future_on::<TokioIo, _, _, _>(move |_ctx| {
                        async move {
                            CounterEvent::SetMessage("Limit reached!".to_string())
                        }
                        .boxed()
                    });
                }
                println!("LOG: {message} - OK (limit: {})", config.max_count);
                return Task::events(vec![]);
            }
        }
        println!("LOG: {message}");
    }
    Task::events(vec![])
}

/// Main effect dispatcher using `Task`
fn handle_effects(
    effect: CounterEffect,
    ctx: &EffectContext<CounterEvent, AppConfig>,
) -> Task<CounterEvent, AppConfig> {
    // Use magic handlers with automatic parameter extraction
    match &effect {
        CounterEffect::LogMessage(msg) if msg.contains("Checking limit") => {
            // Use magic handler with both config and sender extraction
            let config: &AppConfig = ctx.resources();
            handle_limit_check_effect(effect, config)
        }
        CounterEffect::LogMessage(_) => {
            // Simple effect-only magic handler - convert to Task
            handle_log_effect(effect);
            Task::events(vec![])
        }
        CounterEffect::PlaySound => {
            // Magic handler with resource extraction
            let config: &AppConfig = ctx.resources();
            handle_sound_effect(effect, config);
            Task::events(vec![])
        }
        CounterEffect::SaveCount(_) => {
            // Magic handler that returns Task
            handle_save_effect(effect)
        }
    }
}

// ============================================================================
// Step 6: Put it all together
// ============================================================================

#[cfg(feature = "examples")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic TEA Pattern with Magic Handlers Demo ===");
    println!("A simple counter with logging, sound effects, and automatic parameter extraction\n");

    // Build the system with model and resources
    let (core, shell) = Syzygy::builder()
        .model(CounterModel::default())
        .resource(AppConfig {
            max_count: 5,
            enable_sound: true,
        })
        .event_handler(update_counter)
        .effect_handler(handle_effects)
        .build();
    let mut runner = Runner::new(core, shell);

    // Test the magic handlers
    println!("Initial state:");
    let counter: &CounterModel = runner.core().model();
    println!("  Counter: {counter:?}");
    println!("  Config: max_count=5, enable_sound=true\n");

    // Increment to test limit checking
    for i in 1..=6 {
        println!("Step {i}: Incrementing counter");
        runner.core().send_event(CounterEvent::Increment);
        runner.tick(syzygy::spawn::spawner()).await?;

        let counter: &CounterModel = runner.core().model();
        println!("  Count: {}, Message: {}\n", counter.count, counter.message);
    }

    // Test decrement with magic handler
    println!("Testing decrement magic handler:");
    runner.core().send_event(CounterEvent::Decrement);
    runner.tick(syzygy::spawn::spawner()).await?;

    let counter: &CounterModel = runner.core().model();
    println!("  State: {counter:?}\n");

    // Test reset with config access
    println!("Testing reset magic handler (with sound):");
    runner.core().send_event(CounterEvent::Reset);
    runner.tick(syzygy::spawn::spawner()).await?;

    let counter: &CounterModel = runner.core().model();
    println!("  State: {counter:?}\n");

    // Test message setting
    println!("Testing message setting magic handler:");
    runner.core().send_event(CounterEvent::SetMessage(
        "Magic handlers working!".to_string(),
    ));
    runner.tick(syzygy::spawn::spawner()).await?;

    let counter: &CounterModel = runner.core().model();
    println!("  Final state: {counter:?}\n");

    println!("Magic Handlers TEA Key Points:");
    println!("✅ Unidirectional data flow: Event -> Magic Handler -> Model + Effects");
    println!("✅ Automatic parameter extraction - no manual context manipulation");
    println!("✅ Type-safe dependency injection at compile time");
    println!("✅ Clean, focused handlers for each event type");
    println!("✅ Mix and match read/write access patterns as needed");
    println!("✅ Zero runtime overhead - compiles to direct function calls");

    Ok(())
}
