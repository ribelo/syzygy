//! EventMap demonstration with zero-overhead dispatch
//!
//! This example shows how to use the Event derive macro with EventMap
//! for efficient event dispatch using array indexing.

use syzygy::prelude::*;
use std::any::TypeId;

// Define event data types - each must be unique
#[derive(Debug, Clone)]
struct UserCreated {
    id: u64,
    name: String,
}

#[derive(Debug, Clone)]
struct UserUpdated {
    id: u64,
    name: String,
    email: Option<String>,
}

#[derive(Debug, Clone)]
struct UserDeleted {
    id: u64,
    reason: String,
}

#[derive(Debug, Clone)]
struct SystemStarted {
    version: String,
}

#[derive(Debug, Clone)]
struct SystemStopped {
    graceful: bool,
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
    SendEmail { to: String, subject: String },
    LogActivity { message: String },
    UpdateCache { user_id: u64 },
}

// Define model
#[derive(Default)]
struct AppModel {
    users: Vec<(u64, String)>,
    active_users: usize,
    system_running: bool,
}

// Handler functions for each event type
fn handle_user_created(data: &UserCreated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("Handling UserCreated: {:?}", data);
    model.users.push((data.id, data.name.clone()));
    model.active_users += 1;
    
    Dispatch::new(
        vec![],
        vec![
            AppCommand::SendEmail {
                to: format!("admin@example.com"),
                subject: format!("New user: {}", data.name),
            },
            AppCommand::UpdateCache { user_id: data.id },
        ],
    )
}

fn handle_user_updated(data: &UserUpdated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("Handling UserUpdated: {:?}", data);
    
    if let Some(user) = model.users.iter_mut().find(|(id, _)| *id == data.id) {
        user.1 = data.name.clone();
    }
    
    Dispatch::command(AppCommand::UpdateCache { user_id: data.id })
}

fn handle_user_deleted(data: &UserDeleted, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("Handling UserDeleted: {:?}", data);
    
    model.users.retain(|(id, _)| *id != data.id);
    model.active_users = model.active_users.saturating_sub(1);
    
    Dispatch::command(AppCommand::LogActivity {
        message: format!("User {} deleted: {}", data.id, data.reason),
    })
}

fn handle_system_started(data: &SystemStarted, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("Handling SystemStarted: {:?}", data);
    model.system_running = true;
    
    Dispatch::command(AppCommand::LogActivity {
        message: format!("System started v{}", data.version),
    })
}

fn handle_system_stopped(data: &SystemStopped, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    println!("Handling SystemStopped: {:?}", data);
    model.system_running = false;
    
    let message = if data.graceful {
        "System stopped gracefully".to_string()
    } else {
        "System stopped abruptly".to_string()
    };
    
    Dispatch::command(AppCommand::LogActivity { message })
}

fn main() {
    println!("EventMap Demo - Zero-overhead event dispatch\n");
    
    // Create the EventMap and register handlers
    let mut event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on::<UserCreated>(0, handle_user_created)
        .on::<UserUpdated>(1, handle_user_updated)
        .on::<UserDeleted>(2, handle_user_deleted)
        .on::<SystemStarted>(3, handle_system_started)
        .on::<SystemStopped>(4, handle_system_stopped)
        .build();
    
    // Create model
    let mut model = AppModel::default();
    
    // Test events
    let events = vec![
        AppEvent::SystemStarted(SystemStarted {
            version: "1.0.0".to_string(),
        }),
        AppEvent::UserCreated(UserCreated {
            id: 1,
            name: "Alice".to_string(),
        }),
        AppEvent::UserCreated(UserCreated {
            id: 2,
            name: "Bob".to_string(),
        }),
        AppEvent::UserUpdated(UserUpdated {
            id: 1,
            name: "Alice Smith".to_string(),
            email: Some("alice@example.com".to_string()),
        }),
        AppEvent::UserDeleted(UserDeleted {
            id: 2,
            reason: "Account closed".to_string(),
        }),
        AppEvent::SystemStopped(SystemStopped { graceful: true }),
    ];
    
    // Process events
    println!("Processing events...\n");
    for event in &events {
        let result = event_map.dispatch(event, &mut model);
        
        // Display any commands that were generated
        for cmd in result.commands {
            println!("  → Command: {:?}", cmd);
        }
    }
    
    // Display final state
    println!("\nFinal model state:");
    println!("  System running: {}", model.system_running);
    println!("  Active users: {}", model.active_users);
    println!("  Users: {:?}", model.users);
    
    // Demonstrate the From implementations
    println!("\nDemonstrating From implementations:");
    let created_event: AppEvent = UserCreated {
        id: 99,
        name: "Charlie".to_string(),
    }.into();
    println!("  Created via From: {:?}", created_event);
    
    // Show that Event trait is properly implemented
    println!("\nEvent trait details:");
    println!("  AppEvent::LENGTH = {}", AppEvent::LENGTH);
    println!("  UserCreated variant index = {}", created_event.variant_index());
    println!("  Inner TypeId = {:?}", created_event.inner_type_id());
}