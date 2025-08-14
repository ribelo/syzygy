//! Pure functional core example
//!
//! This example demonstrates the pure functional architecture where
//! event handlers are pure functions that return data instead of having side effects.

use syzygy::prelude::*;
use std::collections::HashMap;

// Define our application state
#[derive(Debug, Default)]
struct AppState {
    counter: i32,
    users: HashMap<u32, String>,
    next_user_id: u32,
}

// Define events (input to functional core)
#[derive(Debug, Clone)]
enum AppEvent {
    Increment,
    Decrement,
    CreateUser { name: String },
    DeleteUser { id: u32 },
}

// Define tasks (output from functional core, executed in imperative shell)
#[derive(Debug, Clone)]
enum AppTask {
    LogMessage { message: String },
    SaveCounter { value: i32 },
    SendWelcomeEmail { user_id: u32, name: String },
    AuditUserDeletion { user_id: u32 },
}

// Implement the Task trait for our tasks
impl<E, R> Task<E, R> for AppTask
where
    E: Clone + Send,
    R: Send,
{
    async fn execute(&self, _ctx: TaskContext<E, R>) {
        match self {
            AppTask::LogMessage { message } => {
                println!("LOG: {}", message);
            }
            AppTask::SaveCounter { value } => {
                println!("SAVE: Counter = {}", value);
            }
            AppTask::SendWelcomeEmail { user_id, name } => {
                println!("EMAIL: Welcome email sent to user {} ({})", user_id, name);
            }
            AppTask::AuditUserDeletion { user_id } => {
                println!("AUDIT: User {} was deleted", user_id);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Pure Functional Core Example");
    println!("================================");

    // Build the pure Syzygy system
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(AppState::default())
        .on_event(|event, state| {
            // Pure functional event handler - no side effects!
            // All state changes happen through the mutable state parameter
            // All I/O happens through returned tasks
            match event {
                AppEvent::Increment => {
                    state.counter += 1;
                    Dispatch::new(
                        vec![], // No new events
                        vec![
                            AppTask::LogMessage { 
                                message: format!("Counter incremented to {}", state.counter) 
                            },
                            AppTask::SaveCounter { value: state.counter },
                        ]
                    )
                }
                AppEvent::Decrement => {
                    state.counter -= 1;
                    Dispatch::new(
                        vec![], // No new events
                        vec![
                            AppTask::LogMessage { 
                                message: format!("Counter decremented to {}", state.counter) 
                            },
                            AppTask::SaveCounter { value: state.counter },
                        ]
                    )
                }
                AppEvent::CreateUser { name } => {
                    let user_id = state.next_user_id;
                    state.next_user_id += 1;
                    state.users.insert(user_id, name.clone());
                    
                    Dispatch::new(
                        vec![], // No new events  
                        vec![
                            AppTask::LogMessage { 
                                message: format!("User '{}' created with ID {}", name, user_id) 
                            },
                            AppTask::SendWelcomeEmail { user_id, name },
                        ]
                    )
                }
                AppEvent::DeleteUser { id } => {
                    match state.users.remove(&id) {
                        Some(name) => {
                            Dispatch::new(
                                vec![], // No new events
                                vec![
                                    AppTask::LogMessage { 
                                        message: format!("User '{}' (ID {}) was deleted", name, id) 
                                    },
                                    AppTask::AuditUserDeletion { user_id: id },
                                ]
                            )
                        }
                        None => {
                            Dispatch::new(
                                vec![], // No new events
                                vec![
                                    AppTask::LogMessage { 
                                        message: format!("Attempted to delete non-existent user ID {}", id) 
                                    },
                                ]
                            )
                        }
                    }
                }
            }
        })
        .build();

    println!("\n📝 Testing pure functional operations...");
    
    // Dispatch some events
    handle.dispatch(AppEvent::Increment)?;
    handle.dispatch(AppEvent::Increment)?;
    handle.dispatch(AppEvent::CreateUser { name: "Alice".to_string() })?;
    handle.dispatch(AppEvent::CreateUser { name: "Bob".to_string() })?;
    handle.dispatch(AppEvent::Decrement)?;
    handle.dispatch(AppEvent::DeleteUser { id: 0 })?;
    handle.dispatch(AppEvent::DeleteUser { id: 999 })?; // Non-existent user
    
    // Process all events (this triggers all the tasks)
    syzygy.process_events();
    
    // Check final state
    println!("\n📊 Final state:");
    println!("Counter: {}", syzygy.state().counter);
    println!("Users: {:?}", syzygy.state().users);
    println!("Next user ID: {}", syzygy.state().next_user_id);
    
    println!("\n✅ Pure functional example completed!");
    println!("\n🔍 Key Benefits:");
    println!("   • Pure functions - no side effects in event handlers");
    println!("   • Deterministic - same input always produces same output");
    println!("   • Testable - easy to unit test event handlers");
    println!("   • Separation of concerns - functional core, imperative shell");
    println!("   • Thread-safe - no shared mutable state in handlers");
    
    Ok(())
}