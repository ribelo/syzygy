//! Simple inline future effects for rapid prototyping
//! 
//! This example shows the recommended way to add future effects directly to your
//! effect enum for rapid experimentation before creating proper named effects.
//! 
//! No complex abstractions, no generic wrappers - just add a Future variant!

use std::collections::HashMap;
use std::time::Duration;
use std::hash::{Hash, Hasher, DefaultHasher};
use syzygy::prelude::*;

/// Application events - including error events following error-as-events pattern
#[derive(Debug, Clone)]
enum AppEvent {
    StartExperiment,
    ExperimentComplete { result: String },
    UserLoggedIn { user_id: u32, username: String },
    DataFetched { data: String },
    Shutdown,
    
    // Error events - all errors flow through the event system
    LoginFailed { username: String, reason: String },
    ExperimentFailed { reason: String },
    NetworkError { operation: String, message: String },
    DatabaseError { operation: String, message: String },
    ValidationError { field: String, message: String },
    
    // Recovery events
    RetryOperation { operation: String },
    FallbackActivated { reason: String },
}

/// Effects - mix named effects with a simple Future variant for experimentation  
enum AppEffect {
    // Well-defined named effects (recommended for production)
    Login { username: String, password: String },
    FetchUserData { user_id: u32 },
    Log { level: LogLevel, message: String, metadata: HashMap<String, String> },
    SaveToDatabase { table: String, data: String },
    
    // Simple Future variant for experimentation and prototyping!
    // This is all you need - no complex abstractions required
    Future(Box<dyn FnOnce(EffectContext<AppEvent>) -> futures_util::future::BoxFuture<'static, ()> + Send>),
}

// Custom Debug implementation since closures don't implement Debug
impl std::fmt::Debug for AppEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login { username, password } => f.debug_struct("Login")
                .field("username", username)
                .field("password", &"[REDACTED]")
                .finish(),
            Self::FetchUserData { user_id } => f.debug_struct("FetchUserData")
                .field("user_id", user_id)
                .finish(),
            Self::Log { level, message, metadata } => f.debug_struct("Log")
                .field("level", level)
                .field("message", message)
                .field("metadata", metadata)
                .finish(),
            Self::SaveToDatabase { table, data } => f.debug_struct("SaveToDatabase")
                .field("table", table)
                .field("data", data)
                .finish(),
            Self::Future(_) => f.debug_tuple("Future").field(&"<closure>").finish(),
        }
    }
}

#[derive(Debug, Clone)]
enum LogLevel {
    Info,
    Debug,
    Warn,
    Error,
}

/// Simple resources for the example
#[derive(Debug, Clone)]
struct AppResources {
    pub api_base_url: String,
    pub max_retry_attempts: u32,
}

impl AppResources {
    pub fn new() -> Self {
        Self {
            api_base_url: "https://api.example.com".to_string(),
            max_retry_attempts: 3,
        }
    }
}

// Helper methods for creating effects (optional, but convenient)
impl AppEffect {
    /// Create a future effect for rapid prototyping
    /// 
    /// This is the simple, direct approach - no generics, no Arc, no complexity!
    pub fn future<F>(f: F) -> Self 
    where 
        F: FnOnce(EffectContext<AppEvent>) -> futures_util::future::BoxFuture<'static, ()> + Send + 'static,
    {
        Self::Future(Box::new(f))
    }
}

// Custom Clone implementation since FnOnce can't be cloned
impl Clone for AppEffect {
    fn clone(&self) -> Self {
        match self {
            Self::Login { username, password } => Self::Login { 
                username: username.clone(), 
                password: password.clone() 
            },
            Self::FetchUserData { user_id } => Self::FetchUserData { user_id: *user_id },
            Self::Log { level, message, metadata } => Self::Log { 
                level: level.clone(), 
                message: message.clone(), 
                metadata: metadata.clone() 
            },
            Self::SaveToDatabase { table, data } => Self::SaveToDatabase { 
                table: table.clone(), 
                data: data.clone() 
            },
            Self::Future(_) => panic!("Future effects cannot be cloned - create a new one instead"),
        }
    }
}

/// Application model - includes error state for proper error handling
#[derive(Debug, Default)]
struct ExperimentModel {
    current_user: Option<String>,
    experiment_results: Vec<String>,
    is_running_experiment: bool,
    
    // Error handling state
    last_error: Option<String>,
    retry_count: u32,
    is_fallback_mode: bool,
    failed_operations: Vec<String>,
}

/// Application implementation
#[derive(Default)]
struct ExperimentApp;

impl App for ExperimentApp {
    type Event = AppEvent;
    type Model = ExperimentModel;
    type Effect = AppEffect;
    type Resources = AppResources;

    fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
        match event {
            AppEvent::StartExperiment => {
                model.is_running_experiment = true;
                model.last_error = None;
                
                // Mix named effects with simple inline Future effects
                Command::sequence([
                    Command::effect(AppEffect::Login {
                        username: "alice".to_string(),
                        password: "secret123".to_string(),
                    }),
                    Command::effect(AppEffect::future(|ctx| Box::pin(async move {
                        println!("🧪 Running quick experiment...");
                        
                        // Simulate experimental async work with potential failure
                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_millis(200)).await;
                        
                        // Simulate experiment with 85% success rate
                        use std::time::{SystemTime, UNIX_EPOCH};
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                        let random_value = now.subsec_nanos() % 100;
                        
                        if random_value < 85 {
                            // Success path
                            let _ = ctx.send_event(AppEvent::ExperimentComplete {
                                result: "Experiment shows 42% improvement!".to_string(),
                            });
                            println!("✅ Quick experiment completed");
                        } else {
                            // Failure path - demonstrate error-as-events in future effects
                            let _ = ctx.send_event(AppEvent::ExperimentFailed {
                                reason: "Insufficient data points".to_string(),
                            });
                            println!("❌ Quick experiment failed");
                        }
                    }))),
                    
                    // 3. Another inline effect with captured variables
                    Command::effect(AppEffect::future({
                        let start_time = std::time::Instant::now();
                        move |ctx| {
                            Box::pin(async move {
                                println!("⏱️ Measuring performance...");
                                
                                #[cfg(feature = "tokio")]
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                #[cfg(not(feature = "tokio"))]
                                async_std::task::sleep(Duration::from_millis(100)).await;
                                
                                let elapsed = start_time.elapsed();
                                println!("📊 Performance: {:?}", elapsed);
                                
                                let _ = ctx.send_event(AppEvent::DataFetched {
                                    data: format!("performance_ms:{}", elapsed.as_millis()),
                                });
                            })
                        }
                    })),
                    
                    // 4. Back to named effect
                    Command::effect(AppEffect::SaveToDatabase {
                        table: "experiments".to_string(),
                        data: "experiment_started".to_string(),
                    }),
                ])
            }
            
            AppEvent::UserLoggedIn { user_id, username } => {
                model.current_user = Some(username.clone());
                
                // Parallel effects - mix named and future
                Command::parallel([
                    AppEffect::FetchUserData { user_id },
                    AppEffect::future({
                        let username = username.clone();
                        move |_ctx| {
                            Box::pin(async move {
                                println!("📈 Tracking analytics for {}", username);
                                
                                #[cfg(feature = "tokio")]
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                #[cfg(not(feature = "tokio"))]
                                async_std::task::sleep(Duration::from_millis(50)).await;
                                
                                println!("📊 Analytics tracked for {}", username);
                            })
                        }
                    }),
                    
                    // Named effect
                    AppEffect::Log {
                        level: LogLevel::Info,
                        message: format!("User {} logged in", username),
                        metadata: {
                            let mut map = HashMap::new();
                            map.insert("user_id".to_string(), user_id.to_string());
                            map
                        },
                    },
                ])
            }
            
            AppEvent::ExperimentComplete { result } => {
                model.experiment_results.push(result.clone());
                model.is_running_experiment = false;
                
                // Simple inline effect for notifications
                Command::effect(AppEffect::future(move |_ctx| {
                    Box::pin(async move {
                        println!("🎉 Result: {}", result);
                        println!("📱 Sending notification...");
                    })
                }))
            }
            
            AppEvent::DataFetched { data } => {
                println!("📥 Data: {}", data);
                Command::none()
            }
            
            // Error event handling - demonstrating error-as-events pattern
            AppEvent::LoginFailed { username, reason } => {
                model.last_error = Some(format!("Login failed for {}: {}", username, reason));
                
                // Retry logic with exponential backoff
                if model.retry_count < 3 {
                    model.retry_count += 1;
                    let delay = Duration::from_secs(2_u64.pow(model.retry_count));
                    
                    Command::effect(AppEffect::future(move |ctx| {
                        Box::pin(async move {
                            println!("🔄 Login failed, retrying in {:?}...", delay);
                            
                            #[cfg(feature = "tokio")]
                            tokio::time::sleep(delay).await;
                            #[cfg(not(feature = "tokio"))]
                            async_std::task::sleep(delay).await;
                            
                            let _ = ctx.send_event(AppEvent::RetryOperation { 
                                operation: format!("login:{}", username) 
                            });
                        })
                    }))
                } else {
                    // Max retries exceeded
                    model.retry_count = 0;
                    model.is_fallback_mode = true;
                    model.failed_operations.push(format!("login:{}", username));
                    
                    Command::event(AppEvent::FallbackActivated { 
                        reason: format!("Login failed after {} attempts", model.retry_count + 1)
                    })
                }
            }
            
            AppEvent::ExperimentFailed { reason } => {
                model.last_error = Some(reason.clone());
                model.is_running_experiment = false;
                
                println!("❌ Experiment failed: {}", reason);
                Command::none()
            }
            
            AppEvent::NetworkError { operation, message } => {
                model.last_error = Some(format!("Network error in {}: {}", operation, message));
                
                // Simple retry for network errors
                Command::effect(AppEffect::future(move |ctx| {
                    Box::pin(async move {
                        println!("🌐 Network error: {}, will retry in 5s", message);
                        
                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_secs(5)).await;
                        
                        let _ = ctx.send_event(AppEvent::RetryOperation { operation });
                    })
                }))
            }
            
            AppEvent::DatabaseError { operation, message } => {
                model.last_error = Some(format!("Database error in {}: {}", operation, message));
                model.is_fallback_mode = true;
                
                println!("💾 Database error: {}, enabling fallback mode", message);
                Command::event(AppEvent::FallbackActivated {
                    reason: "Database unavailable".to_string()
                })
            }
            
            AppEvent::ValidationError { field, message } => {
                model.last_error = Some(format!("Validation error in {}: {}", field, message));
                
                // Validation errors usually don't need retries
                println!("⚠️ Validation error in {}: {}", field, message);
                Command::none()
            }
            
            AppEvent::RetryOperation { operation } => {
                println!("🔄 Retrying operation: {}", operation);
                
                // Parse operation and retry appropriately
                if operation.starts_with("login:") {
                    let username = operation.strip_prefix("login:").unwrap_or("unknown");
                    Command::effect(AppEffect::Login {
                        username: username.to_string(),
                        password: "secret123".to_string(), // In real app, store securely or re-prompt
                    })
                } else {
                    println!("Unknown operation to retry: {}", operation);
                    Command::none()
                }
            }
            
            AppEvent::FallbackActivated { reason } => {
                println!("🛡️ Fallback activated: {}", reason);
                model.is_fallback_mode = true;
                
                // In fallback mode, we could use cached data, offline mode, etc.
                Command::effect(AppEffect::future(|_ctx| {
                    Box::pin(async move {
                        println!("📋 Switching to cached/offline data...");
                    })
                }))
            }
            
            AppEvent::Shutdown => {
                // Graceful shutdown
                Command::sequence([
                    Command::effect(AppEffect::SaveToDatabase {
                        table: "sessions".to_string(),
                        data: "session_ended".to_string(),
                    }),
                    Command::effect(AppEffect::future(|_ctx| Box::pin(async move {
                        println!("🧹 Cleanup...");
                        
                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_millis(100)).await;
                        
                        println!("✅ Cleanup completed");
                    }))),
                ])
            }
        }
    }
}

/// Simple effect handler that processes both named and future effects
fn create_effect_handler(effect: AppEffect, resources: std::sync::Arc<AppResources>, ctx: EffectContext<AppEvent>) -> futures_util::future::BoxFuture<'static, ()> {
    // Resources are already in Arc - can move directly into async block
    
    Box::pin(async move {
        match effect {
                // Handle named effects with proper error handling
                AppEffect::Login { username, password: _ } => {
                    println!("🔐 Authenticating: {} (API: {})", username, resources.api_base_url);
                    
                    // Simulate authentication with potential failure
                    #[cfg(feature = "tokio")]
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    #[cfg(not(feature = "tokio"))]
                    async_std::task::sleep(Duration::from_millis(300)).await;
                    
                    // Simulate random failure for demonstration (20% failure rate)
                    let mut hasher = DefaultHasher::new();
                    username.hash(&mut hasher);
                    let hash = hasher.finish();
                    
                    if hash % 5 == 0 {
                        // Simulate authentication failure
                        let _ = ctx.send_event(AppEvent::LoginFailed {
                            username: username.clone(),
                            reason: "Invalid credentials".to_string(),
                        });
                        println!("❌ Authentication failed: {}", username);
                    } else {
                        // Success path
                        let _ = ctx.send_event(AppEvent::UserLoggedIn {
                            user_id: 42,
                            username: username.clone(),
                        });
                        println!("✅ Authenticated: {}", username);
                    }
                }
                
                AppEffect::FetchUserData { user_id } => {
                    println!("👤 Fetching data for user {}", user_id);
                    
                    // Simulate network operation with timeout handling
                    let fetch_future = async {
                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        #[cfg(not(feature = "tokio"))]
                        async_std::task::sleep(Duration::from_millis(150)).await;
                        
                        // Simulate network response
                        format!("user_profile_{}", user_id)
                    };
                    
                    // Add timeout handling
                    let timeout_result = {
                        #[cfg(feature = "tokio")]
                        { tokio::time::timeout(Duration::from_secs(5), fetch_future).await }
                        #[cfg(not(feature = "tokio"))]
                        { async_std::future::timeout(Duration::from_secs(5), fetch_future).await }
                    };
                    
                    match timeout_result {
                        Ok(data) => {
                            let _ = ctx.send_event(AppEvent::DataFetched { data });
                            println!("📥 User data fetched");
                        }
                        Err(_timeout) => {
                            let _ = ctx.send_event(AppEvent::NetworkError {
                                operation: "fetch_user_data".to_string(),
                                message: format!("Timeout fetching data for user {}", user_id),
                            });
                            println!("⏰ Timeout fetching user data");
                        }
                    }
                }
                
                AppEffect::Log { level, message, metadata } => {
                    let level_str = match level {
                        LogLevel::Info => "INFO",
                        LogLevel::Debug => "DEBUG", 
                        LogLevel::Warn => "WARN",
                        LogLevel::Error => "ERROR",
                    };
                    println!("📝 [{}] {} {:?}", level_str, message, metadata);
                }
                
                AppEffect::SaveToDatabase { table, data } => {
                    println!("💾 Saving to {}: {}", table, data);
                    
                    // Simulate database operation with potential failure
                    #[cfg(feature = "tokio")]
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    #[cfg(not(feature = "tokio"))]
                    async_std::task::sleep(Duration::from_millis(100)).await;
                    
                    // Simulate occasional database failure (10% failure rate)
                    let mut hasher = DefaultHasher::new();
                    data.hash(&mut hasher);
                    let hash = hasher.finish();
                    
                    if hash % 10 == 0 {
                        let _ = ctx.send_event(AppEvent::DatabaseError {
                            operation: format!("save_to_{}", table),
                            message: "Connection timeout".to_string(),
                        });
                        println!("❌ Failed to save to {}", table);
                    } else {
                        println!("✅ Saved to {}", table);
                    }
                }
                
                // Handle future effects - just execute them!
                AppEffect::Future(future_fn) => {
                    future_fn(ctx).await;
                }
        }
    })
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
    println!("🚀 Simple Inline Future Effects Demo");
    println!("=====================================");
    println!("💡 No complex abstractions - just add Future variant to your enum!");
    
    // Create resources  
    let resources = AppResources::new();
    
    // Build the system
    let (core, shell) = Syzygy::builder()
        .app(ExperimentApp::default())
        .model(ExperimentModel::default())
        .resources(resources)
        .build();
        
    let shell = shell.with_effect_handler(create_effect_handler);
    let mut runner = Runner::new(core, shell);
    
    // Start the experiment
    runner.core().send_event(AppEvent::StartExperiment)?;
    
    // Run the application
    let start_time = std::time::Instant::now();
    let max_duration = Duration::from_secs(5);
    
    while start_time.elapsed() < max_duration {
        let did_work = runner.tick(syzygy::spawn::spawner()).await?;
        
        if !did_work {
            #[cfg(feature = "tokio")]
            tokio::time::sleep(Duration::from_millis(10)).await;
            #[cfg(not(feature = "tokio"))]
            async_std::task::sleep(Duration::from_millis(10)).await;
        }
        
        if start_time.elapsed() > Duration::from_secs(4) {
            println!("\n🛑 Shutting down...");
            runner.core().send_event(AppEvent::Shutdown)?;
            break;
        }
    }
    
    // Show final state including error information
    println!("\n📊 Final State:");
    let model = runner.core().model();
    println!("  User: {:?}", model.current_user);
    println!("  Results: {:?}", model.experiment_results);
    println!("  Last Error: {:?}", model.last_error);
    println!("  Retry Count: {}", model.retry_count);
    println!("  Fallback Mode: {}", model.is_fallback_mode);
    println!("  Failed Operations: {:?}", model.failed_operations);
    
    println!("\n✅ Demo completed!");
    println!("\n💡 Key Patterns Demonstrated:");
    println!("   🎯 Error-as-Events: All errors flow through the event system");
    println!("   🔄 Retry Logic: Exponential backoff with max retry limits");  
    println!("   🛡️ Fallback Modes: Graceful degradation when services fail");
    println!("   ⚡ Future Effects: Simple inline effects with proper error handling");
    println!("   📝 No Complexity: Just add Future variant to your effect enum!");
    
    println!("\n🏗️ Architecture Benefits:");
    println!("   • Unified error handling through events");
    println!("   • Testable error recovery logic");
    println!("   • No panic-driven development");
    println!("   • Clear separation of concerns");
    
    Ok(())
}
