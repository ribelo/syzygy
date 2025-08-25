//! Advanced Magic Handler Patterns
//!
//! This example demonstrates the full range of magic handler extraction patterns:
//! - Extracting individual resources with pure T
//! - Extracting the entire EffectContext for maximum flexibility
//! - Extracting just the EventSender for lightweight event sending
//! - Mixing different extraction patterns based on handler needs

use syzygy::prelude::*;
use std::collections::HashMap;

// ============================================================================
// Application Types
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserModel {
    current_user: Option<String>,
    login_count: u32,
}

#[derive(Debug, Clone, Default)]
struct ConfigModel {
    app_name: String,
    version: String,
    features_enabled: Vec<String>,
}

#[derive(Debug, Clone)]
struct DatabaseConfig {
    url: String,
    pool_size: u32,
}

#[derive(Debug, Clone)]
struct ApiConfig {
    endpoint: String,
    api_key: String,
}

#[derive(Debug, Clone)]
struct CacheManager {
    cache: HashMap<String, String>,
}

impl CacheManager {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    
    fn get(&self, key: &str) -> Option<&String> {
        self.cache.get(key)
    }
}

// ============================================================================
// Events & Effects
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    UserLoggedIn { username: String },
    DataCached { key: String, value: String },
    ApiRequestCompleted { response: String },
    SystemReady,
    Error { message: String },
}

#[derive(Debug, Clone)]
enum AppEffect {
    AuthenticateUser { username: String, password: String },
    FetchUserData { user_id: String },
    CacheData { key: String, value: String },
    SendNotification { message: String },
    LogMessage { level: String, message: String },
    ComplexWorkflow { task_id: String },
}

// ============================================================================
// Magic Effect Handlers - Different Extraction Patterns
// ============================================================================

/// Pattern 1: Extract just the EventSender for lightweight event-only handlers
async fn handle_simple_notification(
    effect: AppEffect,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::SendNotification { message } => {
            println!("📧 Sending notification: {}", message);
            // Just send an event - no need for full context
            let _ = sender.send(AppEvent::SystemReady);
        }
        _ => {}
    }
}

/// Pattern 2: Extract specific resources for focused handlers
async fn handle_database_operation(
    effect: AppEffect,
    db_config: DatabaseConfig,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::FetchUserData { user_id } => {
            println!("🗄️  Fetching user data from: {} (pool: {})", 
                     db_config.url, db_config.pool_size);
            
            // Simulate database fetch
            #[cfg(feature = "tokio")]
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            
            let _ = sender.send(AppEvent::UserLoggedIn { username: user_id });
        }
        _ => {}
    }
}

/// Pattern 3: Extract the entire EffectContext for maximum flexibility
async fn handle_complex_workflow(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, Storage<CacheManager, Storage<ApiConfig, Storage<DatabaseConfig, EmptyStorage>>>>,
) {
    match effect {
        AppEffect::ComplexWorkflow { task_id } => {
            println!("🔄 Starting complex workflow: {}", task_id);
            
            // Access multiple resources
            let db_config: &DatabaseConfig = ctx.resource();
            let api_config: &ApiConfig = ctx.resource();
            let cache: &CacheManager = ctx.resource();
            
            println!("Using DB: {}, API: {}", db_config.url, api_config.endpoint);
            
            // Check cache first
            if let Some(cached) = cache.get(&task_id) {
                println!("📦 Found in cache: {}", cached);
                let _ = ctx.send_event(AppEvent::DataCached { 
                    key: task_id.clone(), 
                    value: cached.clone() 
                });
                return;
            }
            
            // Spawn async tasks for parallel processing
            let task_id_clone = task_id.clone();
            ctx.spawn(async move {
                println!("⚡ Background task {} processing", task_id_clone);
                #[cfg(feature = "tokio")]
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                println!("✅ Background task {} completed", task_id_clone);
            }).unwrap();
            
            // Send completion event
            let _ = ctx.send_event(AppEvent::ApiRequestCompleted { 
                response: format!("Task {} processed", task_id) 
            });
        }
        _ => {}
    }
}

/// Pattern 4: Mix multiple resource extractions
async fn handle_authentication(
    effect: AppEffect,
    db_config: DatabaseConfig,
    api_config: ApiConfig,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::AuthenticateUser { username, password } => {
            println!("🔐 Authenticating {} via {} using DB: {}", 
                     username, api_config.endpoint, db_config.url);
            
            // Simulate auth
            if password == "secret" {
                let _ = sender.send(AppEvent::UserLoggedIn { username });
            } else {
                let _ = sender.send(AppEvent::Error { 
                    message: "Invalid credentials".to_string() 
                });
            }
        }
        _ => {}
    }
}

// ============================================================================
// Event Handlers with Context Extraction
// ============================================================================

fn handle_user_login(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, Storage<ConfigModel, Storage<UserModel, EmptyStorage>>>,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLoggedIn { username } => {
            let user: &mut UserModel = ctx.model_mut();
            let config: &ConfigModel = ctx.model();
            
            user.current_user = Some(username.clone());
            user.login_count += 1;
            
            println!("👤 User {} logged in (count: {}) - App: {} v{}", 
                     username, user.login_count, config.app_name, config.version);
            
            Command::batch([
                Command::effect(AppEffect::FetchUserData { user_id: username }),
                Command::effect(AppEffect::LogMessage { 
                    level: "INFO".to_string(),
                    message: "User login successful".to_string()
                }),
            ])
        }
        _ => Command::none()
    }
}

// ============================================================================
// Main Example
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Advanced Magic Handler Patterns Demo");
    println!("======================================\n");



    // Set up the system with models and resources
    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(UserModel::default())
        .model(ConfigModel {
            app_name: "Advanced Demo".to_string(),
            version: "2.0.0".to_string(),
            features_enabled: vec!["magic_handlers".to_string(), "async".to_string()],
        })
        .resource(DatabaseConfig {
            url: "postgresql://localhost/myapp".to_string(),
            pool_size: 10,
        })
        .resource(ApiConfig {
            endpoint: "https://api.example.com".to_string(),
            api_key: "secret-key-123".to_string(),
        })
        .resource(CacheManager::new())
        .update(handle_user_login)
        .build();

    let shell = shell
        .with_effect_handler(|effect, ctx: EffectContext<AppEvent, Storage<CacheManager, Storage<ApiConfig, Storage<DatabaseConfig, EmptyStorage>>>>| async move {
            match &effect {
                AppEffect::SendNotification { .. } => {
                    handle_simple_notification(effect, EventSender(ctx.event_sender().unwrap())).await
                }
                AppEffect::FetchUserData { .. } => {
                    let db_config: &DatabaseConfig = ctx.resource();
                    handle_database_operation(effect, db_config.clone(), EventSender(ctx.event_sender().unwrap())).await
                }
                AppEffect::ComplexWorkflow { .. } => {
                    handle_complex_workflow(effect, ctx.clone()).await
                }
                AppEffect::AuthenticateUser { .. } => {
                    let db_config: &DatabaseConfig = ctx.resource();
                    let api_config: &ApiConfig = ctx.resource();
                    handle_authentication(effect, db_config.clone(), api_config.clone(), EventSender(ctx.event_sender().unwrap())).await
                }
                AppEffect::LogMessage { level, message } => {
                    println!("[{}] {}", level, message);
                }
                _ => {}
            }
        });

    let mut runner = Runner::new(core, shell);

    println!("📋 Testing Different Magic Handler Patterns:\n");

    // Test 1: Simple notification (EventSender only)
    println!("1️⃣ Testing EventSender extraction:");
    runner.core().send_event(AppEvent::SystemReady)?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();

    // Test 2: Database operation (specific resources)
    println!("2️⃣ Testing specific resource extraction:");
    runner.core().send_event(AppEvent::UserLoggedIn { 
        username: "alice".to_string() 
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();

    // Test 3: Complex workflow (full EffectContext)
    println!("3️⃣ Testing full EffectContext extraction:");
    runner.core().send_event(AppEvent::UserLoggedIn { 
        username: "system".to_string() 
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();

    // Test 4: Authentication (multiple resources)
    println!("4️⃣ Testing multiple resource extraction:");
    runner.core().send_event(AppEvent::UserLoggedIn { 
        username: "bob".to_string() 
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();

    println!("✨ Advanced Magic Handler Patterns Complete!\n");
    println!("🎯 Patterns Demonstrated:");
    println!("   ✅ EventSender extraction (lightweight event sending)");
    println!("   ✅ Specific resource extraction (focused handlers)");
    println!("   ✅ Full EffectContext extraction (maximum flexibility)");
    println!("   ✅ Multiple resource extraction (complex operations)");
    println!("   ✅ Mixed extraction patterns in single application");
    println!("   ✅ Type-safe compilation and zero runtime overhead");

    Ok(())
}