//! UnsafeEventMap demonstration with maximum performance dispatch
//!
//! This example shows how to use the UnsafeEventMap with Event derive macro
//! for the fastest possible event dispatch using raw function pointers.

use syzygy::prelude::*;

// Define unique event data types
#[derive(Debug, Clone)]
struct UserCreated {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug, Clone)]
struct UserUpdated {
    id: u64,
    name: String,
    department: Option<String>,
}

#[derive(Debug, Clone)]
struct UserDeleted {
    id: u64,
    reason: String,
}

#[derive(Debug, Clone)]
struct SystemStarted {
    version: String,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct SystemStopped {
    graceful: bool,
    uptime_seconds: u64,
}

// Define the event enum with Event derive
#[derive(Debug, Clone, Event)]
enum AppEvent {
    UserCreated(UserCreated),
    UserUpdated(UserUpdated),
    UserDeleted(UserDeleted),
    SystemStarted(SystemStarted),
    SystemStopped(SystemStopped),
}

// Define commands
#[derive(Debug)]
enum AppCommand {
    SendWelcomeEmail { user_id: u64, email: String },
    UpdateUserIndex { user_id: u64 },
    LogEvent { message: String },
    NotifyAdmins { event: String },
}

// Define model
#[derive(Default, Debug)]
struct AppModel {
    users: Vec<(u64, String, String)>, // id, name, email
    active_users: usize,
    system_running: bool,
    total_events: usize,
}

// Handler functions that receive concrete types directly - no downcasting!
fn handle_user_created(data: UserCreated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("🆕 User created: {} ({})", data.name, data.email);
    
    model.users.push((data.id, data.name.clone(), data.email.clone()));
    model.active_users += 1;
    model.total_events += 1;
    
    Dispatch::new(
        vec![], // No follow-up events
        vec![
            AppCommand::SendWelcomeEmail {
                user_id: data.id,
                email: data.email,
            },
            AppCommand::UpdateUserIndex { user_id: data.id },
        ],
    )
}

fn handle_user_updated(data: UserUpdated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("📝 User updated: {} (dept: {:?})", data.name, data.department);
    
    if let Some(user) = model.users.iter_mut().find(|(id, _, _)| *id == data.id) {
        user.1 = data.name.clone();
    }
    model.total_events += 1;
    
    Dispatch::command(AppCommand::UpdateUserIndex { user_id: data.id })
}

fn handle_user_deleted(data: UserDeleted, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("🗑️  User deleted: {} (reason: {})", data.id, data.reason);
    
    model.users.retain(|(id, _, _)| *id != data.id);
    model.active_users = model.active_users.saturating_sub(1);
    model.total_events += 1;
    
    Dispatch::command(AppCommand::LogEvent {
        message: format!("User {} deleted: {}", data.id, data.reason),
    })
}

fn handle_system_started(data: SystemStarted, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("🚀 System started: v{} at {}", data.version, data.timestamp);
    
    model.system_running = true;
    model.total_events += 1;
    
    Dispatch::command(AppCommand::NotifyAdmins {
        event: format!("System v{} started", data.version),
    })
}

fn handle_system_stopped(data: SystemStopped, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    let status = if data.graceful { "gracefully" } else { "unexpectedly" };
    println!("🛑 System stopped {} after {}s", status, data.uptime_seconds);
    
    model.system_running = false;
    model.total_events += 1;
    
    let message = format!(
        "System stopped {} after {} seconds uptime",
        status, data.uptime_seconds
    );
    
    Dispatch::command(AppCommand::LogEvent { message })
}

fn main() {
    println!("🚀 UnsafeEventMap Demo - Maximum Performance Event Dispatch\n");
    
    // Build the unsafe event map - note the unsafe blocks
    let event_map = unsafe {
        UnsafeEventMapBuilder::<AppEvent, AppModel, AppCommand>::new()
            .on::<UserCreated>(0, handle_user_created)       // Variant index 0
            .on::<UserUpdated>(1, handle_user_updated)       // Variant index 1
            .on::<UserDeleted>(2, handle_user_deleted)       // Variant index 2
            .on::<SystemStarted>(3, handle_system_started)   // Variant index 3
            .on::<SystemStopped>(4, handle_system_stopped)   // Variant index 4
            .build()
    };
    
    let mut model = AppModel::default();
    
    // Create test events
    let events = vec![
        AppEvent::SystemStarted(SystemStarted {
            version: "2.0.0".to_string(),
            timestamp: 1692123456,
        }),
        AppEvent::UserCreated(UserCreated {
            id: 1,
            name: "Alice Johnson".to_string(),
            email: "alice@company.com".to_string(),
        }),
        AppEvent::UserCreated(UserCreated {
            id: 2,
            name: "Bob Smith".to_string(),
            email: "bob@company.com".to_string(),
        }),
        AppEvent::UserUpdated(UserUpdated {
            id: 1,
            name: "Alice Johnson".to_string(),
            department: Some("Engineering".to_string()),
        }),
        AppEvent::UserUpdated(UserUpdated {
            id: 2,
            name: "Bob Smith".to_string(),
            department: Some("Marketing".to_string()),
        }),
        AppEvent::UserDeleted(UserDeleted {
            id: 2,
            reason: "Left company".to_string(),
        }),
        AppEvent::SystemStopped(SystemStopped {
            graceful: true,
            uptime_seconds: 86400, // 24 hours
        }),
    ];
    
    println!("Processing {} events with zero-cost dispatch...\n", events.len());
    
    // Process all events - note the unsafe dispatch calls
    for (i, event) in events.into_iter().enumerate() {
        println!("Event {}: {:?}", i + 1, event);
        
        unsafe {
            let result = event_map.dispatch(event, &mut model);
            
            // Display generated commands
            if !result.commands.is_empty() {
                for cmd in result.commands {
                    println!("  → Generated command: {:?}", cmd);
                }
            }
            
            // Display generated events (if any)
            if !result.events.is_empty() {
                for evt in result.events {
                    println!("  → Generated event: {:?}", evt);
                }
            }
        }
        
        println!();
    }
    
    // Display final state
    println!("📊 Final Model State:");
    println!("  System running: {}", model.system_running);
    println!("  Active users: {}", model.active_users);
    println!("  Total events processed: {}", model.total_events);
    println!("  Users in database:");
    for (id, name, email) in &model.users {
        println!("    {} - {} ({})", id, name, email);
    }
    
    // Demonstrate the From implementations work
    println!("\n🔧 Demonstrating automatic From implementations:");
    let user_event: AppEvent = UserCreated {
        id: 99,
        name: "Test User".to_string(),
        email: "test@example.com".to_string(),
    }.into();
    println!("  Created via From: {:?}", user_event);
    
    // Show Event trait implementation details
    println!("\n📈 Event Trait Details:");
    println!("  AppEvent::LENGTH = {}", AppEvent::LENGTH);
    println!("  UserCreated variant index = {}", user_event.variant_index());
    println!("  Inner TypeId = {:?}", user_event.inner_type_id());
    
    // Performance notes
    println!("\n⚡ Performance Notes:");
    println!("  ✅ Zero allocations - handlers stored as raw pointers");
    println!("  ✅ Zero dynamic dispatch - direct function calls");
    println!("  ✅ Zero downcasting - handlers receive concrete types");
    println!("  ✅ Array indexing - O(1) handler lookup");
    println!("  ✅ Should be nearly identical to match statement performance!");
    
    println!("\n🚨 Safety Notes:");
    println!("  ⚠️  Uses unsafe code for maximum performance");
    println!("  ⚠️  Handler types must match variant data types exactly");
    println!("  ⚠️  Derive macro ensures type safety at compile time");
    println!("  ⚠️  Only use when performance is absolutely critical");
}