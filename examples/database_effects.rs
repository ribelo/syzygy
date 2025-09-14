//! Database effects example - showing proper data flow patterns
//!
//! This demonstrates how to handle database operations without state capture,
//! following the Crux lessons about pure effect handlers.

use std::collections::HashMap;
use std::time::Duration;
use syzygy::executor::{ExecutorRegistry, Task, TokioIo};
use syzygy::prelude::*;

use futures::FutureExt;

/// Events - including database results
#[derive(Debug, Clone)]
enum AppEvent {
    ConnectDatabase { connection_string: String },
    DatabaseConnected,
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
    SaveUser { user: User, table: String },
    /// Connect to database with specific config
    ConnectDatabase { connection_string: String },
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
}

impl AppResources {
    pub fn new() -> Self {
        Self {
            database_url: "sqlite://database.db".to_string(),
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

fn database_update(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppModel>,
) -> Command<AppEvent, AppEffect> {
    let model: &mut AppModel = ctx.model_mut();
    match event {
        AppEvent::ConnectDatabase { connection_string } => {
            // Set connecting state and trigger connection effect
            model.is_connected = false; // Will be set to true when connection succeeds
            Command::effect(AppEffect::ConnectDatabase { connection_string })
        }

        AppEvent::DatabaseConnected => {
            model.is_connected = true;
            model.last_error = None;
            Command::none()
        }

        AppEvent::LoadUser { user_id } => {
            // Check if we already have the user in local state
            if let Some(user) = model.users.get(&user_id) {
                // User already loaded - return event directly
                Command::event(AppEvent::UserLoaded {
                    user_id,
                    user: user.clone(),
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
                message: format!("User {} loaded: {}", user_id, user.name),
            })
        }

        AppEvent::UserNotFound { user_id } => {
            model.last_error = Some(format!("User {user_id} not found"));
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
                message: format!("User {user_id} saved successfully"),
            })
        }

        AppEvent::DatabaseError { operation, error } => {
            model.last_error = Some(format!("{operation}: {error}"));
            Command::effect(AppEffect::Log {
                message: format!("Database error in {operation}: {error}"),
            })
        }
    }
}

fn handle_effects(
    effect: AppEffect,
    ctx: &EffectContext<AppEvent, AppResources>,
) -> Task<AppEvent, AppResources> {
    match effect {
        AppEffect::GetUser { user_id, table } => {
            let resources: &AppResources = ctx.resources();
            let resources = resources.clone();
            Task::async_task_with::<TokioIo, _, _>(
                move |_ctx: EffectContext<AppEvent, AppResources>| {
                    let table = table.clone();
                    async move {
                        println!(
                            "🔍 Getting user {user_id} from table {table} (db: {})",
                            resources.database_url
                        );

                        // Simulate database query
                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_millis(100)).await;

                        // Simulate database lookup
                        match simulate_database_get(&table, user_id) {
                            Ok(Some(user)) => {
                                // Success - return user loaded event
                                println!("✅ User {user_id} found");
                                Outcome::Event(AppEvent::UserLoaded { user_id, user })
                            }
                            Ok(None) => {
                                // User not found
                                println!("❌ User {user_id} not found");
                                Outcome::Event(AppEvent::UserNotFound { user_id })
                            }
                            Err(db_error) => {
                                // Database error
                                println!("💥 Database error getting user {user_id}");
                                Outcome::Event(AppEvent::DatabaseError {
                                    operation: format!("get_user_{user_id}"),
                                    error: db_error,
                                })
                            }
                        }
                    }
                    .boxed()
                },
            )
        }

        AppEffect::SaveUser { user, table } => {
            Task::async_task_with::<TokioIo, _, _>(
                move |_ctx: EffectContext<AppEvent, AppResources>| {
                    let user = user.clone();
                    let table = table.clone();
                    async move {
                        println!("💾 Saving user {} to table {}", user.id, table);

                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_millis(150)).await;

                        // Simulate database save
                        match simulate_database_save(&table, &user) {
                            Ok(()) => {
                                println!("✅ User {} saved", user.id);
                                Outcome::Event(AppEvent::UserSaved { user_id: user.id })
                            }
                            Err(db_error) => {
                                println!("💥 Database error saving user {}", user.id);
                                Outcome::Event(AppEvent::DatabaseError {
                                    operation: format!("save_user_{}", user.id),
                                    error: db_error,
                                })
                            }
                        }
                    }
                    .boxed()
                },
            )
        }

        AppEffect::ConnectDatabase { connection_string } => {
            Task::async_task_with::<TokioIo, _, _>(
                move |_ctx: EffectContext<AppEvent, AppResources>| {
                    let connection_string = connection_string.clone();
                    async move {
                        println!("🔌 Connecting to database: {connection_string}");

                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_millis(200)).await;

                        // In real app, you'd establish connection here
                        println!("✅ Database connected");
                        Outcome::Event(AppEvent::DatabaseConnected)
                    }
                    .boxed()
                },
            )
        }

        AppEffect::Log { message } => {
            println!("📝 {message}");
            Task::events(vec![])
        }
    }
}

/// Simulated database operations - in real app these would be `SQLx`, Diesel, etc.
fn simulate_database_get(_table: &str, user_id: u32) -> Result<Option<User>, String> {
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
        999 => Err("Database connection timeout".to_string()), // Simulate error
        _ => Ok(None),                                         // User not found
    }
}

fn simulate_database_save(_table: &str, user: &User) -> Result<(), String> {
    // Simulate validation
    if user.name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    if user.id == 666 {
        return Err("Database constraint violation".to_string()); // Simulate error
    }

    Ok(())
}

#[cfg(feature = "examples")]
#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_demo()
}

#[cfg(feature = "examples")]
#[cfg(all(not(feature = "tokio"), feature = "async-std"))]
#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_demo()
}

#[cfg(feature = "examples")]
#[cfg(all(not(feature = "tokio"), not(feature = "async-std")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("This example requires either 'tokio' or 'async-std' feature");
    Ok(())
}

fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
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
    let mut registry = ExecutorRegistry::new();
    registry.insert_async(TokioIo::default());

    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .resource(resources)
        .event_handler(database_update)
        .effect_handler(handle_effects)
        .with_executor_registry(registry)
        .build();
    let mut runner = Runner::new(core, shell);

    // Connect to database first
    println!("🚀 Step 1: Connecting to database...");
    runner.core().send_event(AppEvent::ConnectDatabase {
        connection_string: "postgresql://localhost/demo".to_string(),
    });
    runner.step()?;

    // Load a user
    println!("🚀 Step 2: Loading user data...");
    runner.core().send_event(AppEvent::LoadUser { user_id: 1 }); // Will find Alice
    runner.step()?;
    runner.step()?;

    // Try to load a user that exists
    println!("\n🔍 Loading existing user...");
    runner.core().send_event(AppEvent::LoadUser { user_id: 2 }); // Will find Bob
    runner.step()?;
    runner.step()?;

    // Try to load a user that doesn't exist
    println!("\n🔍 Loading non-existent user...");
    runner.core().send_event(AppEvent::LoadUser { user_id: 42 }); // Won't find
    runner.step()?;
    runner.step()?;

    // Try to load a user that causes database error
    println!("\n🔍 Triggering database error...");
    runner
        .core()
        .send_event(AppEvent::LoadUser { user_id: 999 }); // Will error
    runner.step()?;
    runner.step()?;

    // Save a new user
    println!("\n💾 Saving new user...");
    let new_user = User {
        id: 3,
        name: "Charlie".to_string(),
        email: "charlie@example.com".to_string(),
    };
    runner
        .core()
        .send_event(AppEvent::SaveUser { user: new_user });
    runner.step()?;
    runner.step()?;

    // Try to save a user that will cause an error
    println!("\n💾 Triggering save error...");
    let bad_user = User {
        id: 666,
        name: "Evil User".to_string(),
        email: "evil@example.com".to_string(),
    };
    runner
        .core()
        .send_event(AppEvent::SaveUser { user: bad_user });
    runner.step()?;
    runner.step()?;

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
