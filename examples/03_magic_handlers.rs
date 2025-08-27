//! Example 03: Magic Handlers
//!
//! This example demonstrates Syzygy's magic handler system for automatic parameter extraction.
//! You'll learn:
//! - Automatic parameter extraction from contexts
//! - Different extraction patterns (models, resources, senders)
//! - Type-safe dependency injection similar to Axum
//! - Clean handler composition

use syzygy::prelude::*;

// ============================================================================
// Models and Resources
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

#[derive(Debug, Clone)]
struct DatabaseConfig {
    url: String,
    pool_size: u32,
}

// ============================================================================
// Events & Effects
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    UserLogin { email: String },
    UserLogout,
    UpdateConfig { debug_mode: bool },
    SystemReady,
}

#[derive(Debug, Clone)]
enum AppEffect {
    SaveUser { email: String },
    LogActivity { message: String },
    DatabaseQuery { query: String },
    SendNotification { message: String },
}

// ============================================================================
// Magic Event Handlers - Automatic Parameter Extraction
// ============================================================================

/// Handler 1: Simple event-only handler (no extraction)
fn handle_simple_event(event: AppEvent) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::SystemReady => {
            println!("System is ready!");
            Command::effect(AppEffect::LogActivity {
                message: "System initialized".to_string(),
            })
        }
        _ => Command::none(),
    }
}

/// Handler 2: Extract a single model (read-only)
fn handle_with_user_model(event: AppEvent, user: &UserModel) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogin { email } => {
            println!(
                "Login attempt for {} (current user: {}, count: {})",
                email, user.name, user.login_count
            );
            
            Command::effect(AppEffect::SaveUser { email })
        }
        _ => Command::none(),
    }
}

/// Handler 3: Extract multiple models (mixed read/write access)
fn handle_with_multiple_models(
    event: AppEvent,
    user: &mut UserModel,
    config: &ConfigModel,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogin { email } => {
            // Modify user model
            user.email = email.clone();
            user.name = email.split('@').next().unwrap_or("user").to_string();
            user.login_count += 1;
            
            println!(
                "User {} logged in to {} v{} (count: {})",
                user.name, config.app_name, config.version, user.login_count
            );
            
            Command::batch([
                Command::effect(AppEffect::SaveUser { email }),
                Command::effect(AppEffect::LogActivity {
                    message: format!("Login successful for {}", user.name),
                }),
            ])
        }
        _ => Command::none(),
    }
}

/// Handler 4: Extract config model for updates
fn handle_config_update(
    event: AppEvent,
    config: &mut ConfigModel,
    user: &UserModel,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UpdateConfig { debug_mode } => {
            let old_debug = config.debug_mode;
            config.debug_mode = debug_mode;
            
            println!(
                "User {} changed debug mode from {} to {} in {}",
                user.name, old_debug, debug_mode, config.app_name
            );
            
            Command::effect(AppEffect::LogActivity {
                message: format!("Debug mode changed to {}", debug_mode),
            })
        }
        _ => Command::none(),
    }
}

// ============================================================================
// Magic Effect Handlers - Resource Extraction
// ============================================================================

/// Effect handler with EventSender extraction
async fn handle_with_event_sender(
    effect: AppEffect,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::SendNotification { message } => {
            println!("Sending notification: {}", message);
            // Send follow-up event
            let _ = sender.send(AppEvent::SystemReady);
        }
        _ => {}
    }
}

/// Effect handler with resource extraction
async fn handle_with_database(
    effect: AppEffect,
    db_config: &DatabaseConfig,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::DatabaseQuery { query } => {
            println!(
                "Executing query '{}' on {} (pool: {})",
                query, db_config.url, db_config.pool_size
            );
            
            // Simulate database operation
            #[cfg(feature = "tokio")]
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            
            // Send completion event
            let _ = sender.send(AppEvent::SystemReady);
        }
        _ => {}
    }
}

/// Effect handler with full context access
async fn handle_with_full_context(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, Storage<DatabaseConfig, EmptyStorage>>,
) {
    match effect {
        AppEffect::SaveUser { email } => {
            let db_config: &DatabaseConfig = ctx.resource();
            
            println!(
                "Saving user {} to database {} with pool size {}",
                email, db_config.url, db_config.pool_size
            );
            
            // Spawn background task
            ctx.spawn(async move {
                println!("Background: Processing user save for {}", email);
                #[cfg(feature = "tokio")]
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                println!("Background: User {} saved successfully", email);
            }).unwrap();
        }
        _ => {}
    }
}

// ============================================================================
// Main Update Function - Dispatch to Magic Handlers
// ============================================================================

fn update_app(
    event: AppEvent,
    ctx: &mut EventContext<
        AppEvent,
        AppEffect,
        Storage<ConfigModel, Storage<UserModel, EmptyStorage>>,
    >,
) -> Command<AppEvent, AppEffect> {
    // Dispatch to appropriate magic handler based on event type
    match event {
        AppEvent::SystemReady => {
            event_trigger(event, ctx, handle_simple_event)
        }
        AppEvent::UserLogin { .. } => {
            // Use the multi-model handler for login
            event_trigger(event, ctx, handle_with_multiple_models)
        }
        AppEvent::UpdateConfig { .. } => {
            event_trigger(event, ctx, handle_config_update)
        }
        AppEvent::UserLogout => {
            // Use single model handler for logout
            event_trigger(event, ctx, handle_with_user_model)
        }
    }
}

// ============================================================================
// Effect Dispatcher
// ============================================================================

async fn handle_effects(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, Storage<DatabaseConfig, EmptyStorage>>,
) {
    match &effect {
        AppEffect::LogActivity { message } => {
            println!("LOG: {}", message);
        }
        AppEffect::SendNotification { .. } => {
            let sender = EventSender(ctx.event_sender().unwrap());
            handle_with_event_sender(effect, sender).await;
        }
        AppEffect::DatabaseQuery { .. } => {
            let db_config: &DatabaseConfig = ctx.resource();
            let sender = EventSender(ctx.event_sender().unwrap());
            handle_with_database(effect, db_config, sender).await;
        }
        AppEffect::SaveUser { .. } => {
            handle_with_full_context(effect, ctx).await;
        }
    }
}

// ============================================================================
// Main Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Magic Handlers Demo ===");
    println!("Demonstrating automatic parameter extraction\n");
    
    // Build system with models and resources
    let (core, shell) = Syzygy::builder()
        .model(UserModel::default())
        .model(ConfigModel {
            app_name: "Magic Demo".to_string(),
            version: "1.0.0".to_string(),
            debug_mode: false,
        })
        .resource(DatabaseConfig {
            url: "postgresql://localhost/demo".to_string(),
            pool_size: 5,
        })
        .update(update_app)
        .build();
    
    let shell = shell.with_effect_handler(handle_effects);
    let mut runner = Runner::new(core, shell);
    
    println!("Testing different magic handler patterns:\n");
    
    // Test 1: Simple handler (event only)
    println!("1. Testing simple handler (event only)");
    runner.core().send_event(AppEvent::SystemReady)?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();
    
    // Test 2: Multi-model handler (with mutations)
    println!("2. Testing multi-model handler (with mutations)");
    runner.core().send_event(AppEvent::UserLogin {
        email: "alice@example.com".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    let user: &UserModel = runner.core().model();
    println!("   User state: {:?}\n", user);
    
    // Test 3: Config update handler
    println!("3. Testing config update handler");
    runner.core().send_event(AppEvent::UpdateConfig { debug_mode: true })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    let config: &ConfigModel = runner.core().model();
    println!("   Config state: {:?}\n", config);
    
    // Test 4: Effect handlers with resource extraction
    println!("4. Testing effect handlers with resources");
    runner.core().send_event(AppEvent::UserLogin {
        email: "bob@example.com".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();
    
    // Give background tasks time to complete
    #[cfg(feature = "tokio")]
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    
    println!("Magic Handlers Key Points:");
    println!("✅ Automatic parameter extraction like Axum");
    println!("✅ Type-safe dependency injection at compile time");
    println!("✅ Mix and match extraction patterns as needed");
    println!("✅ Zero runtime overhead - compiles to direct calls");
    println!("✅ Clean, focused handler functions");
    
    Ok(())
}
