//! Example 01: Basic TEA Pattern
//!
//! This example demonstrates the fundamental TEA (The Elm Architecture) pattern in Syzygy.
//! You'll learn:
//! - Basic app structure with model, events, and effects
//! - Simple update functions
//! - Event processing and command creation
//! - Core/Shell orchestration with Runner

use syzygy::prelude::*;

// ============================================================================
// Step 1: Define your application state (Model)
// ============================================================================

#[derive(Debug, Default)]
struct CounterModel {
    count: i32,
    message: String,
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
}

// ============================================================================
// Step 3: Define side effects your app can trigger
// ============================================================================

#[derive(Debug, Clone)]
enum CounterEffect {
    LogMessage(String),
    PlaySound,
}

// ============================================================================
// Step 4: Write your update function (the heart of TEA)
// ============================================================================

fn update_counter(
    event: CounterEvent,
    ctx: &mut EventContext<CounterEvent, CounterEffect, Storage<CounterModel, EmptyStorage>>,
) -> Command<CounterEvent, CounterEffect> {
    let model: &mut CounterModel = ctx.model_mut();
    
    match event {
        CounterEvent::Increment => {
            model.count += 1;
            model.message = format!("Count incremented to {}", model.count);
            
            Command::batch([
                Command::effect(CounterEffect::LogMessage(model.message.clone())),
                Command::effect(CounterEffect::PlaySound),
            ])
        }
        
        CounterEvent::Decrement => {
            model.count -= 1;
            model.message = format!("Count decremented to {}", model.count);
            
            Command::effect(CounterEffect::LogMessage(model.message.clone()))
        }
        
        CounterEvent::Reset => {
            model.count = 0;
            model.message = "Counter reset".to_string();
            
            Command::effect(CounterEffect::LogMessage("Counter was reset".to_string()))
        }
        
        CounterEvent::SetMessage(message) => {
            model.message = message;
            Command::none()
        }
    }
}

// ============================================================================
// Step 5: Handle side effects
// ============================================================================

async fn handle_effects(effect: CounterEffect, _ctx: EffectContext<CounterEvent, EmptyStorage>) {
    match effect {
        CounterEffect::LogMessage(message) => {
            println!("LOG: {message}");
        }
        CounterEffect::PlaySound => {
            println!("BEEP! (sound effect)");
        }
    }
}

// ============================================================================
// Step 6: Put it all together
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic TEA Pattern Demo ===");
    println!("A simple counter with logging and sound effects\n");
    
    // Build the system
    let (core, shell) = Syzygy::builder()
        .model(CounterModel::default())
        .update(update_counter)
        .build();
    
    let shell = shell.with_effect_handler(handle_effects);
    let mut runner = Runner::new(core, shell);
    
    // Test the counter
    println!("Initial state: {:?}\n", runner.core().model());
    
    // Increment a few times
    for i in 1..=3 {
        println!("Step {i}: Incrementing counter");
        runner.core().send_event(CounterEvent::Increment)?;
        runner.tick(syzygy::spawn::spawner()).await?;
        println!("State: {:?}\n", runner.core().model());
    }
    
    // Decrement
    println!("Decrementing counter");
    runner.core().send_event(CounterEvent::Decrement)?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!("State: {:?}\n", runner.core().model());
    
    // Reset
    println!("Resetting counter");
    runner.core().send_event(CounterEvent::Reset)?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!("State: {:?}\n", runner.core().model());
    
    // Set custom message
    println!("Setting custom message");
    runner.core().send_event(CounterEvent::SetMessage("Custom message!".to_string()))?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!("Final state: {:?}\n", runner.core().model());
    
    println!("TEA Pattern Key Points:");
    println!("✅ Unidirectional data flow: Event -> Update -> Model + Effects");
    println!("✅ Pure update function: No side effects, just model updates + commands");
    println!("✅ Clear separation: Core (sync) handles state, Shell (async) handles effects");
    println!("✅ Predictable: Same event always produces same model change");
    
    Ok(())
}
