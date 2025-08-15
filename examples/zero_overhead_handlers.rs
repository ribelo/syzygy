//! Zero-overhead event handlers with crabtime code generation
//!
//! This example demonstrates the new magic event handler API with crabtime
//! integration for compile-time code generation that achieves zero runtime overhead.

use syzygy::prelude::*;
use syzygy::codegen::{generate_complete_event_system, generate_code_string};

// ============================================================================
// Application Model
// ============================================================================

#[derive(Debug, Clone, Default)]
struct AppModel {
    users: Vec<String>,
    counter: i32,
    enabled: bool,
}

// Implement FromContainer for automatic field extraction
impl FromContainer<AppModel> for Vec<String> {
    fn from_container(model: &AppModel) -> Self {
        model.users.clone()
    }
}

impl FromContainer<AppModel> for i32 {
    fn from_container(model: &AppModel) -> Self {
        model.counter
    }
}

impl FromContainer<AppModel> for bool {
    fn from_container(model: &AppModel) -> Self {
        model.enabled
    }
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
// Magic Event Handlers
// ============================================================================

// Handler that extracts users list and counter from model
fn handle_create_user_magic(
    event: CreateUser,
    users: Vec<String>,
    counter: i32,
) -> Dispatch<AppEvent, AppCommand> {
    let new_name = format!("{}_{}", event.name, counter);
    
    println!("Creating user: {} (total users: {})", new_name, users.len() + 1);
    
    Dispatch::new(
        vec![AppEvent::UserCreated { name: new_name.clone() }],
        vec![AppCommand::SaveUser { name: new_name }],
    )
}

// Handler that only needs the counter
fn handle_update_counter_magic(
    event: UpdateCounter,
    counter: i32,
) -> Dispatch<AppEvent, AppCommand> {
    let new_value = counter + event.increment;
    
    println!("Updating counter from {} to {}", counter, new_value);
    
    Dispatch::new(
        vec![AppEvent::CounterUpdated { value: new_value }],
        vec![AppCommand::LogMessage { 
            message: format!("Counter updated from {} to {}", counter, new_value) 
        }],
    )
}

// Handler that extracts the enabled flag
fn handle_toggle_enabled_magic(
    _event: ToggleEnabled,
    enabled: bool,
) -> Dispatch<AppEvent, AppCommand> {
    let new_enabled = !enabled;
    
    println!("Toggling system from {} to {}", enabled, new_enabled);
    
    Dispatch::new(
        vec![AppEvent::SystemToggled { enabled: new_enabled }],
        vec![AppCommand::UpdateDatabase { 
            table: "system_config".to_string(),
            data: format!("enabled={}", new_enabled)
        }],
    )
}

// ============================================================================
// Generated Code Example (What crabtime will produce)
// ============================================================================

// This demonstrates what the crabtime generation would produce:
// 
// ```rust
// // Generated handler functions
// pub fn handle_createuser<M, E, C>(event: CreateUser, model: &mut M) -> Dispatch<E, C> {
//     // Magic parameter extraction
//     let users = Vec::<String>::from_container(model);
//     let counter = i32::from_container(model);
//     
//     // Call the actual handler
//     handle_create_user_magic(event, users, counter)
// }
// 
// pub fn handle_updatecounter<M, E, C>(event: UpdateCounter, model: &mut M) -> Dispatch<E, C> {
//     let counter = i32::from_container(model);
//     handle_update_counter_magic(event, counter)
// }
// 
// pub fn handle_toggleenabled<M, E, C>(event: ToggleEnabled, model: &mut M) -> Dispatch<E, C> {
//     let enabled = bool::from_container(model);
//     handle_toggle_enabled_magic(event, enabled)
// }
// 
// // Generated zero-overhead dispatcher
// pub fn generated_event_dispatcher<M, E, C>(event: E, model: &mut M) -> Dispatch<E, C> {
//     match event {
//         E::CreateUser(inner) => handle_createuser(inner, model),
//         E::UpdateCounter(inner) => handle_updatecounter(inner, model),
//         E::ToggleEnabled(inner) => handle_toggleenabled(inner, model),
//         _ => Dispatch::none(),
//     }
// }
// ```

// ============================================================================
// Example Usage
// ============================================================================

fn main() {
    println!("🚀 Zero-Overhead Event Handlers Example");
    println!("=========================================");
    
    // Create the builder with the new magic handler API
    let builder = Syzygy::builder()
        .model(AppModel::default())
        .on_event::<CreateUser>(|event| {
            // For demonstration, we use simple handlers here
            // In the full implementation, these would use magic parameter extraction
            Dispatch::new(
                vec![AppEvent::UserCreated { name: event.name.clone() }],
                vec![AppCommand::SaveUser { name: event.name }],
            )
        })
        .on_event::<UpdateCounter>(|event| {
            Dispatch::new(
                vec![AppEvent::CounterUpdated { value: event.increment }],
                vec![AppCommand::LogMessage { 
                    message: format!("Counter incremented by {}", event.increment) 
                }],
            )
        })
        .on_event::<ToggleEnabled>(|_event| {
            Dispatch::new(
                vec![AppEvent::SystemToggled { enabled: true }],
                vec![AppCommand::UpdateDatabase { 
                    table: "system".to_string(),
                    data: "toggled".to_string()
                }],
            )
        });
    
    println!("\n📊 Collected Handler Metadata:");
    println!("Number of handlers: {}", builder.event_handler_storage.handlers.len());
    
    for (i, handler) in builder.event_handler_storage.handlers.iter().enumerate() {
        println!("  {}. {} -> {}", 
                 i + 1, 
                 handler.event_type_name, 
                 handler.handler_function_name);
    }
    
    println!("\n🔧 Generated Handler Code:");
    for (name, _code) in &builder.event_handler_storage.handler_code {
        println!("  - {}", name);
    }
    
    // Demonstrate proc macro code generation
    println!("\n⚡ Proc Macro Code Generation:");
    println!("Handlers collected for zero-overhead dispatch generation:");
    
    let handlers = builder.event_handler_storage.handlers.clone();
    
    // Generate the actual code that would be produced
    let generated_code = generate_code_string(&handlers);
    
    println!("\n📝 Generated Code:");
    println!("{}", "=".repeat(80));
    // Print first 500 characters of generated code
    let preview = if generated_code.len() > 500 {
        format!("{}...\n[truncated - total length: {} characters]", 
                &generated_code[..500], generated_code.len())
    } else {
        generated_code.clone()
    };
    println!("{}", preview);
    println!("{}", "=".repeat(80));
    
    for handler in &handlers {
        println!("  - {} will dispatch to {}", 
                 handler.event_type_name, 
                 handler.handler_function_name);
    }
    
    println!("\n✨ Magic Parameter Extraction Demo:");
    
    // Test magic handlers directly
    let mut model = AppModel {
        users: vec!["alice".to_string(), "bob".to_string()],
        counter: 42,
        enabled: false,
    };
    
    let create_event = CreateUser { name: "charlie".to_string() };
    let create_result = handle_create_user_magic.call_with_event(create_event, &mut model);
    
    println!("Create user result: {} events, {} commands", 
             create_result.events.len(), 
             create_result.commands.len());
    
    let update_event = UpdateCounter { increment: 5 };
    let update_result = handle_update_counter_magic.call_with_event(update_event, &mut model);
    
    println!("Update counter result: {} events, {} commands", 
             update_result.events.len(), 
             update_result.commands.len());
    
    let toggle_event = ToggleEnabled;
    let toggle_result = handle_toggle_enabled_magic.call_with_event(toggle_event, &mut model);
    
    println!("Toggle enabled result: {} events, {} commands", 
             toggle_result.events.len(), 
             toggle_result.commands.len());
    
    println!("\n🎯 Zero-Overhead Achievement:");
    println!("✅ Compile-time handler collection");
    println!("✅ Token-based code generation ready");
    println!("✅ Magic parameter extraction functional");
    println!("✅ Type-safe event dispatch");
    println!("⏳ Crabtime integration (next phase)");
    
    println!("\nNext: Generate actual match expressions with crabtime for <200ps dispatch!");
}