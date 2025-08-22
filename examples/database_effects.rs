//! Database effects example - showing proper data flow patterns
//! 
//! This demonstrates how to handle database operations without state capture,
//! following the Crux lessons about pure effect handlers.

use std::collections::HashMap;
use syzygy::prelude::*;
use std::time::Duration;

/// Events - including database results
#[derive(Debug, Clone)]
enum AppEvent {
    LoadUser { user_id: u32 },
    UserLoaded { user_id: u32, user: User },
    UserNotFound { user_id: u32 },
    SaveUser { user: User },
    UserSaved { user_id: u32 },
    DatabaseError { operation: String, error: String },
}

/// Effects - describe what database operations to perform  
#[derive(Debug, Clone)]
enum AppEffect {
    /// Get user from database - all data needed is in the effect
    GetUser { 
        user_id: u32,
        /// Could include connection config, table name, etc.
        table: String,
    },
    /// Save user to database - all data needed is in the effect
    SaveUser { 
        user: User,
        table: String,
    },
    /// Connect to database with specific config
    ConnectDatabase { 
        connection_string: String,
    },
    /// Log a message
    Log { message: String },
}

/// User data structure
#[derive(Debug, Clone)]
struct User {
    id: u32,
    name: String,
    email: String,
}

/// Resources - dependencies available to effect handlers
#[derive(Debug, Clone)]
struct AppResources {
    pub database_url: String,
    pub max_connections: u32,
    pub timeout_ms: u64,
}

impl AppResources {
    pub fn new() -> Self {
        Self {
            database_url: "sqlite://database.db".to_string(),
            max_connections: 10,
            timeout_ms: 5000,
        }
    }
}

/// App model - holds application state
#[derive(Debug, Default)]
struct AppModel {
    users: HashMap<u32, User>,
    is_connected: bool,
    last_error: Option<String>,
}

/// App implementation
#[derive(Default)]
struct DatabaseApp;

impl App for DatabaseApp {
    type Event = AppEvent;
    type Model = AppModel;
    type Effect = AppEffect;
    type Resources = AppResources;

    fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
        match event {
            AppEvent::LoadUser { user_id } => {
                // Check if we already have the user in local state
                if let Some(user) = model.users.get(&user_id) {
                    // User already loaded - return event directly
                    Command::event(AppEvent::UserLoaded { 
                        user_id, 
                        user: user.clone() 
                    })
                } else {
                    // Need to fetch from database - create effect with all needed data
                    Command::effect(AppEffect::GetUser { 
                        user_id,
                        table: "users".to_string(), // Could come from config
                    })
                }
            }

            AppEvent::UserLoaded { user_id, user } => {
                // Store user in local state
                model.users.insert(user_id, user.clone());
                model.last_error = None;
                
                Command::effect(AppEffect::Log { 
                    message: format!("User {} loaded: {}", user_id, user.name) 
                })
            }

            AppEvent::UserNotFound { user_id } => {
                model.last_error = Some(format!("User {} not found", user_id));
                Command::none()
            }

            AppEvent::SaveUser { user } => {
                // Save to database - effect has all the data it needs
                Command::effect(AppEffect::SaveUser { 
                    user: user.clone(),
                    table: "users".to_string(),
                })
            }

            AppEvent::UserSaved { user_id } => {
                model.last_error = None;
                Command::effect(AppEffect::Log { 
                    message: format!("User {} saved successfully", user_id) 
                })
            }

            AppEvent::DatabaseError { operation, error } => {
                model.last_error = Some(format!("{}: {}", operation, error));
                Command::effect(AppEffect::Log { 
                    message: format!("Database error in {}: {}", operation, error) 
                })
            }
        }
    }
}

/// Effect handler - receives all data through parameters
/// NO STATE CAPTURE - this is the key lesson from Crux!
/// Resources provide clean access to typed dependencies via Arc for 'static compatibility
fn handle_effects(effect: AppEffect, resources: std::sync::Arc<AppResources>, ctx: EffectContext<AppEvent>) -> futures_util::future::BoxFuture<'static, ()> {
    // Resources are already in Arc - can move directly into async block
    
    Box::pin(async move {
        match effect {
            AppEffect::GetUser { user_id, table } => {
                println!("🔍 Getting user {} from table {}", user_id, table);
                
                // Use resources for database configuration
                println!("📊 Using DB: {} (max_conn: {}, timeout: {}ms)", 
                    resources.database_url, resources.max_connections, resources.timeout_ms);
                
                // Simulate database query with resource-provided timeout
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(resources.timeout_ms / 50)).await;
                #[cfg(not(feature = "tokio"))]
                async_std::task::sleep(Duration::from_millis(resources.timeout_ms / 50)).await;
                
                // Simulate database lookup - resources provide connection info
                match simulate_database_get(&table, user_id, &*resources).await {
                    Ok(Some(user)) => {
                        // Success - send user loaded event
                        let _ = ctx.send_event(AppEvent::UserLoaded { user_id, user });
                        println!("✅ User {} found", user_id);
                    }
                    Ok(None) => {
                        // User not found
                        let _ = ctx.send_event(AppEvent::UserNotFound { user_id });
                        println!("❌ User {} not found", user_id);
                    }
                    Err(db_error) => {
                        // Database error
                        let _ = ctx.send_event(AppEvent::DatabaseError {
                            operation: format!("get_user_{}", user_id),
                            error: db_error,
                        });
                        println!("💥 Database error getting user {}", user_id);
                    }
                }
            }

            AppEffect::SaveUser { user, table } => {
                println!("💾 Saving user {} to table {}", user.id, table);
                
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(150)).await;
                #[cfg(not(feature = "tokio"))]
                async_std::task::sleep(Duration::from_millis(150)).await;
                
                // Simulate database save with resource configuration
                match simulate_database_save(&table, &user, &*resources).await {
                    Ok(()) => {
                        let _ = ctx.send_event(AppEvent::UserSaved { user_id: user.id });
                        println!("✅ User {} saved", user.id);
                    }
                    Err(db_error) => {
                        let _ = ctx.send_event(AppEvent::DatabaseError {
                            operation: format!("save_user_{}", user.id),
                            error: db_error,
                        });
                        println!("💥 Database error saving user {}", user.id);
                    }
                }
            }

            AppEffect::ConnectDatabase { connection_string } => {
                println!("🔌 Connecting to database: {} (from resources: {})", 
                    connection_string, resources.database_url);
                
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(200)).await;
                #[cfg(not(feature = "tokio"))]
                async_std::task::sleep(Duration::from_millis(200)).await;
                
                // In real app, you'd establish connection here
                println!("✅ Database connected");
            }

            AppEffect::Log { message } => {
                println!("📝 {}", message);
            }
        }
    })
}

/// Simulated database operations - in real app these would be SQLx, Diesel, etc.
async fn simulate_database_get(_table: &str, user_id: u32, resources: &AppResources) -> Result<Option<User>, String> {
    // Simulate some users existing
    match user_id {
        1 => Ok(Some(User {
            id: 1,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        })),
        2 => Ok(Some(User {
            id: 2,
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
        })),
        999 => Err(format!("Database connection timeout ({}ms)", resources.timeout_ms)), // Simulate error
        _ => Ok(None), // User not found
    }
}

async fn simulate_database_save(_table: &str, user: &User, _resources: &AppResources) -> Result<(), String> {
    // Simulate validation
    if user.name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    
    if user.id == 666 {
        return Err("Database constraint violation".to_string()); // Simulate error
    }
    
    Ok(())
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_demo().await
}

#[cfg(all(not(feature = "tokio"), feature = "async-std"))]
#[async_std::main] 
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_demo().await
}

#[cfg(all(not(feature = "tokio"), not(feature = "async-std")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("This example requires either 'tokio' or 'async-std' feature");
    Ok(())
}

async fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
    println!("🗄️  Database Effects Demo");
    println!("========================");
    println!("📚 Key Patterns:");
    println!("  • Effects contain all needed data (no state capture)");
    println!("  • Database results flow back through events"); 
    println!("  • Pure effect handlers - testable in isolation");
    println!("  • Error handling through events, not exceptions");
    println!();
    
    // Create resources
    let resources = AppResources::new();

    // Build the system with resources
    let (core, shell) = Syzygy::builder()
        .app(DatabaseApp::default())
        .model(AppModel::default())
        .resources(resources)
        .build();
        
    let shell = shell.with_effect_handler(handle_effects);
    let mut runner = Runner::new(core, shell);
    
    // Connect to database first
    runner.core().send_event(AppEvent::LoadUser { user_id: 1 })?; // Will find Alice
    runner.tick(syzygy::spawn::spawner()).await?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Try to load a user that exists
    println!("\n🔍 Loading existing user...");
    runner.core().send_event(AppEvent::LoadUser { user_id: 2 })?; // Will find Bob  
    runner.tick(syzygy::spawn::spawner()).await?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Try to load a user that doesn't exist
    println!("\n🔍 Loading non-existent user...");
    runner.core().send_event(AppEvent::LoadUser { user_id: 42 })?; // Won't find
    runner.tick(syzygy::spawn::spawner()).await?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Try to load a user that causes database error
    println!("\n🔍 Triggering database error...");
    runner.core().send_event(AppEvent::LoadUser { user_id: 999 })?; // Will error
    runner.tick(syzygy::spawn::spawner()).await?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Save a new user
    println!("\n💾 Saving new user...");
    let new_user = User {
        id: 3,
        name: "Charlie".to_string(),
        email: "charlie@example.com".to_string(),
    };
    runner.core().send_event(AppEvent::SaveUser { user: new_user })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Try to save a user that will cause an error
    println!("\n💾 Triggering save error...");
    let bad_user = User {
        id: 666,
        name: "Evil User".to_string(),
        email: "evil@example.com".to_string(),
    };
    runner.core().send_event(AppEvent::SaveUser { user: bad_user })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Show final state
    println!("\n📊 Final State:");
    let model = runner.core().model();
    println!("  Loaded users: {}", model.users.len());
    for (id, user) in &model.users {
        println!("    {}: {} ({})", id, user.name, user.email);
    }
    println!("  Last error: {:?}", model.last_error);
    
    println!("\n✅ Demo completed!");
    println!("\n💡 Key Takeaways:");
    println!("   🎯 Effects receive ALL needed data as parameters");
    println!("   📨 Database results come back as events, not return values"); 
    println!("   🧪 Effect handlers are pure functions - easy to test");
    println!("   ⚠️  Errors become events, maintaining unified data flow");
    println!("   🔄 No state capture in effects - follows Crux best practices");
    
    Ok(())
}
