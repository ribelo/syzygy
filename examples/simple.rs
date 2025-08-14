//! Simple example showing the clean, no-magic Syzygy API
//! 
//! This example demonstrates the "error-as-event" pattern where validation
//! errors and failures are represented as events in the enum rather than
//! exceptions or Result types. This keeps the event processing deterministic
//! and allows for sophisticated error handling and recovery workflows.

use syzygy::prelude::*;

// Define your state
#[derive(Debug, Default)]
struct AppState {
    users: Vec<String>,
    count: i32,
}

// Define a simple resource
#[derive(Debug, Default)]
struct Logger {
    logs: Vec<String>,
}

impl Logger {
    fn log(&self, message: &str) {
        println!("Logger: {}", message);
    }
}

// Define your event enum - including error events
#[derive(Debug, Clone)]
enum AppEvent {
    CreateUser { name: String },
    IncrementCounter,
    DecrementCounter { amount: i32 },
    
    // Error events - errors are just events in the enum
    UserCreationFailed { reason: String },
    ValidationError { field: String, message: String },
}

// Define your task enum
#[derive(Debug, Clone)]
enum AppTask {
    LogUserCreated { name: String },
    CounterUpdated { new_value: i32 },
    SaveToDatabase { data: String },
}

// Commands are handled by command handler functions (if async execution is needed)

// Event handler function - must be a function, not a closure with captures
fn handle_events(
    event: AppEvent, 
    model: &mut AppState
) -> Dispatch<AppEvent, AppTask> {
    match event {
        AppEvent::CreateUser { name } => {
            // Validate user name - demonstrate error-as-event pattern
            if name.trim().is_empty() {
                // Return error event instead of panicking
                return Dispatch::event(AppEvent::ValidationError { 
                    field: "name".to_string(),
                    message: "Name cannot be empty".to_string() 
                });
            }
            
            if name.len() > 50 {
                // Another validation error as event
                return Dispatch::event(AppEvent::ValidationError { 
                    field: "name".to_string(),
                    message: "Name too long (max 50 chars)".to_string() 
                });
            }
            
            // Check for duplicate users
            if model.users.contains(&name) {
                // User already exists - error as event
                return Dispatch::event(AppEvent::UserCreationFailed { 
                    reason: format!("User '{}' already exists", name) 
                });
            }
            
            model.users.push(name.clone());
            
            // Return successful tasks to execute
            Dispatch::new(
                vec![], // No follow-up events
                vec![
                    AppTask::LogUserCreated { name: name.clone() },
                    AppTask::SaveToDatabase { 
                        data: format!("user:{}", name) 
                    },
                ]
            )
        }
        AppEvent::IncrementCounter => {
            model.count += 1;
            
            Dispatch::new(
                vec![], // No follow-up events  
                vec![AppTask::CounterUpdated { 
                    new_value: model.count 
                }]
            )
        }
        AppEvent::DecrementCounter { amount } => {
            model.count -= amount;
            
            Dispatch::new(
                vec![], // No follow-up events
                vec![AppTask::CounterUpdated { 
                    new_value: model.count 
                }]
            )
        }
        
        // Handle error events - demonstrate error-as-event pattern
        AppEvent::ValidationError { field, message } => {
            println!("⚠️  Validation Error in field '{}': {}", field, message);
            // Could trigger logging tasks or recovery actions
            Dispatch::command(AppTask::LogUserCreated { 
                name: format!("VALIDATION_ERROR: {} - {}", field, message) 
            })
        }
        
        AppEvent::UserCreationFailed { reason } => {
            println!("❌ User Creation Failed: {}", reason);
            // Could trigger retry logic or alternative flows
            Dispatch::command(AppTask::LogUserCreated { 
                name: format!("CREATION_FAILED: {}", reason) 
            })
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple Syzygy Example");
    println!("=======================");

    // Build the Syzygy system
    let (mut syzygy, handle) = Syzygy::builder()
        .resource(Logger::default()) // Logger resource (must come before model)
        .model(AppState::default())  // Single model 
        .event_handler(handle_events)
        .build();
    
    println!("\n📝 Dispatching events (including error cases)...");
    
    // Send some events - including error cases to demonstrate error-as-event pattern
    handle.dispatch(AppEvent::CreateUser { 
        name: "Alice".to_string() 
    })?;
    
    // Try to create user with empty name - will trigger validation error event
    handle.dispatch(AppEvent::CreateUser { 
        name: "   ".to_string() // Empty name after trim
    })?;
    
    // Try to create duplicate user - will trigger creation failed event
    handle.dispatch(AppEvent::CreateUser { 
        name: "Alice".to_string() // Duplicate
    })?;
    
    // Try to create user with too long name - will trigger validation error event
    handle.dispatch(AppEvent::CreateUser { 
        name: "A".repeat(60) // Too long
    })?;
    
    handle.dispatch(AppEvent::IncrementCounter)?;
    
    handle.dispatch(AppEvent::DecrementCounter { 
        amount: 5 
    })?;
    
    // Process events (including error events)
    syzygy.process_events();
    
    // Check final state
    println!("\n📊 Final state:");
    let model: &AppState = syzygy.model();
    println!("Users: {:?}", model.users);
    println!("Counter: {}", model.count);
    
    println!("\n✅ Simple example completed!");
    
    Ok(())
}