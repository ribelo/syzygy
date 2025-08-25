//! Magic Handlers Demonstration
//!
//! This example showcases the magic handler system that enables automatic
//! parameter extraction for event handlers, similar to Axum's dependency injection.
//!
//! The magic handler system allows handlers to declare exactly what data
//! they need, and the system automatically extracts it from the EventContext.

use syzygy::prelude::*;

// ============================================================================
// Application Models
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserModel {
    name: String,
    email: String,
    login_count: u32,
}

#[derive(Debug, Clone, Default)]
struct ConfigModel {
    app_name: String,
    version: String,
    debug_mode: bool,
}

// ============================================================================
// Events & Effects
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    UserLogin { email: String },
    UserLogout { email: String },
    UpdateConfig { debug_mode: bool },
}

#[derive(Debug, Clone)]
enum AppEffect {
    SaveUser { email: String, login_count: u32 },
    LogActivity { message: String },
    UpdateDatabase { query: String },
}

// ============================================================================
// Magic Event Handlers
// ============================================================================

/// Simple handler with just the event - no extraction
fn handle_simple_event(event: AppEvent) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogout { email } => {
            println!("👋 User {} logging out (simple handler)", email);
            Command::effect(AppEffect::LogActivity {
                message: format!("User {} logged out", email),
            })
        }
        _ => Command::none(),
    }
}

/// Handler that extracts a single model
fn handle_with_user_model(
    event: AppEvent,
    user: &UserModel,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogin { email } => {
            println!(
                "🔐 User {} logging in (current user: {}, login count: {})",
                email, user.name, user.login_count
            );
            
            Command::batch([
                Command::effect(AppEffect::SaveUser {
                    email: email.clone(),
                    login_count: user.login_count + 1,
                }),
                Command::effect(AppEffect::LogActivity {
                    message: format!("User {} logged in", email),
                }),
            ])
        }
        _ => Command::none(),
    }
}

/// Handler that extracts multiple models
fn handle_with_multiple_models(
    event: AppEvent,
    user: &UserModel,
    config: &ConfigModel,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UpdateConfig { debug_mode } => {
            println!(
                "⚙️ Updating config for user {} in app {} v{} (debug: {} -> {})",
                user.name, config.app_name, config.version, config.debug_mode, debug_mode
            );
            
            Command::effect(AppEffect::UpdateDatabase {
                query: format!("UPDATE config SET debug_mode = {} WHERE user = '{}'", debug_mode, user.email),
            })
        }
        _ => Command::none(),
    }
}

/// Handler using mutable model reference
fn handle_with_mutable_model(
    event: AppEvent,
    user: &mut UserModel,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogin { email } => {
            println!("🔓 Processing login for {} (mutable handler)", email);
            
            // Update the model directly
            user.login_count += 1;
            user.email = email.clone();
            
            Command::effect(AppEffect::LogActivity {
                message: format!("Login processed, count now: {}", user.login_count),
            })
        }
        _ => Command::none(),
    }
}

// ============================================================================
// Main Demonstration
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Magic Handlers Demonstration");
    println!("==============================");
    println!("This example shows how magic handlers automatically extract parameters.\n");

    // Create storage with multiple models
    let mut storage = EmptyStorage
        .with_model(UserModel {
            name: "alice".to_string(),
            email: "alice@example.com".to_string(),
            login_count: 5,
        })
        .with_model(ConfigModel {
            app_name: "Demo App".to_string(),
            version: "1.0.0".to_string(),
            debug_mode: false,
        });

    println!("📋 Testing Magic Event Handlers:");
    println!("================================\n");

    // Create event context
    let event_ctx = EventContext::<AppEvent, AppEffect, _>::new(&mut storage);

    // Test 1: Simple handler with no extraction
    println!("1️⃣ Testing simple handler (event only):");
    let logout_event = AppEvent::UserLogout {
        email: "bob@example.com".to_string(),
    };
    let command = event_trigger(logout_event, &event_ctx, handle_simple_event);
    println!("   Generated {} command steps\n", command.into_iter().count());

    // Test 2: Handler with single model extraction
    println!("2️⃣ Testing single model extraction:");
    let login_event = AppEvent::UserLogin {
        email: "bob@example.com".to_string(),
    };
    let command = event_trigger(login_event, &event_ctx, handle_with_user_model);
    println!("   Generated {} command steps\n", command.into_iter().count());

    // Test 3: Handler with multiple model extractions
    println!("3️⃣ Testing multiple model extraction:");
    let config_event = AppEvent::UpdateConfig { debug_mode: true };
    let command = event_trigger(config_event, &event_ctx, handle_with_multiple_models);
    println!("   Generated {} command steps\n", command.into_iter().count());

    // Test 4: Handler with mutable model access
    println!("4️⃣ Testing mutable model extraction:");
    let login_event2 = AppEvent::UserLogin {
        email: "charlie@example.com".to_string(),
    };
    let command = event_trigger(login_event2, &event_ctx, handle_with_mutable_model);
    println!("   Generated {} command steps\n", command.into_iter().count());

    println!("✨ Magic Handler Demonstration Complete!\n");
    println!("🎯 Key Features Demonstrated:");
    println!("   ✅ Simple event handlers (no extraction)");
    println!("   ✅ Single model extraction with &T");
    println!("   ✅ Multiple model extraction");
    println!("   ✅ Mutable model access with &mut T");
    println!("   ✅ Type-safe extraction with compile-time verification");
    println!("   ✅ Zero-overhead parameter injection");

    Ok(())
}