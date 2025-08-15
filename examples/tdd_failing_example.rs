//! TDD RED phase: Failing example that shows the desired API
//!
//! This example demonstrates what we WANT to achieve - building a Syzygy system
//! using ONLY on_event<T>() handlers without requiring a manual event_handler.
//! 
//! Current state: FAILS TO COMPILE - this is intentional (RED phase)
//! Next: Make it work (GREEN phase)
//! Then: Refactor (REFACTOR phase)

use syzygy::prelude::*;

// ============================================================================
// Application Model
// ============================================================================

#[derive(Debug, Clone, Default)]
struct AppModel {
    users: Vec<String>,
    counter: i32,
    enabled: bool,
}

// ============================================================================
// Events and Commands
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
struct CreateUser {
    name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct UpdateCounter {
    increment: i32,
}

#[derive(Debug, Clone, PartialEq)]
struct ToggleEnabled;

#[derive(Debug, Clone, PartialEq)]
enum AppEvent {
    CreateUser(CreateUser),
    UpdateCounter(UpdateCounter),
    ToggleEnabled(ToggleEnabled),
    UserCreated { name: String },
    CounterUpdated { value: i32 },
    SystemToggled { enabled: bool },
}

#[derive(Debug, Clone, PartialEq)]
enum AppCommand {
    SaveUser { name: String },
    LogMessage { message: String },
    UpdateDatabase { table: String, data: String },
}

// Implement From traits for event conversion
impl From<CreateUser> for AppEvent {
    fn from(event: CreateUser) -> Self {
        AppEvent::CreateUser(event)
    }
}

impl From<UpdateCounter> for AppEvent {
    fn from(event: UpdateCounter) -> Self {
        AppEvent::UpdateCounter(event)
    }
}

impl From<ToggleEnabled> for AppEvent {
    fn from(event: ToggleEnabled) -> Self {
        AppEvent::ToggleEnabled(event)
    }
}

impl TryFrom<AppEvent> for CreateUser {
    type Error = ();
    
    fn try_from(event: AppEvent) -> Result<Self, Self::Error> {
        match event {
            AppEvent::CreateUser(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

impl TryFrom<AppEvent> for UpdateCounter {
    type Error = ();
    
    fn try_from(event: AppEvent) -> Result<Self, Self::Error> {
        match event {
            AppEvent::UpdateCounter(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

impl TryFrom<AppEvent> for ToggleEnabled {
    type Error = ();
    
    fn try_from(event: AppEvent) -> Result<Self, Self::Error> {
        match event {
            AppEvent::ToggleEnabled(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

// ============================================================================
// TDD RED: The API we want to achieve
// ============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔴 TDD RED Phase: Failing Example");
    println!("==================================");
    println!("Goal: Build Syzygy with ONLY on_event<T>() handlers");
    println!("Current state: Should FAIL to compile (this is expected!)");
    
    // This is the API we want to achieve:
    // Build a Syzygy system using ONLY on_event<T>() calls
    // WITHOUT requiring a separate event_handler function
    let (mut syzygy, handle, _executor) = Syzygy::builder()
        .model(AppModel::default())
        
        // Register typed event handlers - these should be the ONLY handlers needed
        .on_event::<CreateUser>(|event| {
            println!("📝 Creating user: {}", event.name);
            Dispatch::new(
                vec![AppEvent::UserCreated { name: event.name.clone() }],
                vec![AppCommand::SaveUser { name: event.name }],
            )
        })
        
        .on_event::<UpdateCounter>(|event| {
            println!("🔢 Updating counter by: {}", event.increment);
            Dispatch::new(
                vec![AppEvent::CounterUpdated { value: event.increment }],
                vec![AppCommand::LogMessage { 
                    message: format!("Counter incremented by {}", event.increment) 
                }],
            )
        })
        
        .on_event::<ToggleEnabled>(|_event| {
            println!("🔄 Toggling system state");
            Dispatch::new(
                vec![AppEvent::SystemToggled { enabled: true }],
                vec![AppCommand::UpdateDatabase { 
                    table: "system".to_string(),
                    data: "toggled".to_string()
                }],
            )
        })
        
        // THIS IS THE KEY: We want to build WITHOUT calling .event_handler()
        // The system should auto-generate the event handler from the on_event calls
        .build(); // <-- This should work but currently FAILS
    
    println!("✅ If this compiles, we've achieved the goal!");
    
    // Test the system
    println!("\n🧪 Testing the auto-generated event system:");
    
    let test_events = vec![
        AppEvent::CreateUser(CreateUser { name: "alice".to_string() }),
        AppEvent::UpdateCounter(UpdateCounter { increment: 5 }),
        AppEvent::ToggleEnabled(ToggleEnabled),
    ];
    
    for event in test_events {
        println!("\n📨 Dispatching: {:?}", event);
        handle.dispatch(event)?;
        syzygy.process_events();
        
        println!("📊 Model: users={}, counter={}, enabled={}", 
                syzygy.model().users.len(),
                syzygy.model().counter,
                syzygy.model().enabled);
    }
    
    println!("\n🎯 TDD Success: Auto-generated event handlers working!");
    
    Ok(())
}