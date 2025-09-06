//! Example 05: Real-World Application
//!
//! This example demonstrates a complete real-world application using all Syzygy features.
//! You'll learn:
//! - Complex application architecture
//! - Error handling patterns
//! - State management across multiple domains
//! - Production-ready effect handling
//! - Testing and debugging strategies
//!
//! Note: This is an architectural demonstration. Many types and fields are included
//! to show complete application structure but not all are used in the demo.

// Suppress warnings for architectural demo elements that aren't fully implemented
#![allow(dead_code)]

use std::collections::HashMap;
use syzygy::prelude::*;
use syzygy::executor::{TokioExecutor, ExecutorStorage, EmptyExecutorStorage};
use syzygy::streaming::EffectResult;

// ============================================================================
// Domain Models - Separate concerns
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserState {
    current_user: Option<User>,
    login_attempts: u32,
    session_token: Option<String>,
}

#[derive(Debug, Clone)]
struct User {
    id: u32,
    username: String,
    email: String,
    role: UserRole,
    preferences: UserPreferences,
}

#[derive(Debug, Clone)]
enum UserRole {
    Admin,
    User,
    Guest,
}

#[derive(Debug, Clone, Default)]
struct UserPreferences {
    theme: String,
    language: String,
    notifications_enabled: bool,
}

#[derive(Debug, Clone, Default)]
struct AppState {
    is_loading: bool,
    error_message: Option<String>,
    notification_queue: Vec<Notification>,
    active_operations: HashMap<String, OperationStatus>,
}

#[derive(Debug, Clone)]
struct Notification {
    id: String,
    message: String,
    level: NotificationLevel,
    timestamp: u64,
}

#[derive(Debug, Clone)]
enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone)]
struct OperationStatus {
    operation_id: String,
    status: String,
    progress: f32,
}

#[derive(Debug, Clone, Default)]
struct DataState {
    cache: HashMap<String, CachedItem>,
    last_sync: Option<u64>,
    pending_changes: Vec<DataChange>,
}

#[derive(Debug, Clone)]
struct CachedItem {
    data: String,
    timestamp: u64,
    ttl_seconds: u64,
}

#[derive(Debug, Clone)]
struct DataChange {
    id: String,
    change_type: ChangeType,
    data: String,
}

#[derive(Debug, Clone)]
enum ChangeType {
    Create,
    Update,
    Delete,
}

// ============================================================================
// Application Resources
// ============================================================================

#[derive(Debug, Clone)]
struct DatabaseService {
    connection_string: String,
    max_connections: u32,
    retry_attempts: u32,
}

impl DatabaseService {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User, String> {
        println!("DB: Authenticating user {}", username);

        // Simulate database lookup
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        if username == "admin" && password == "secret" {
            Ok(User {
                id: 1,
                username: username.to_string(),
                email: "admin@example.com".to_string(),
                role: UserRole::Admin,
                preferences: UserPreferences {
                    theme: "dark".to_string(),
                    language: "en".to_string(),
                    notifications_enabled: true,
                },
            })
        } else if username == "user" && password == "password" {
            Ok(User {
                id: 2,
                username: username.to_string(),
                email: "user@example.com".to_string(),
                role: UserRole::User,
                preferences: UserPreferences::default(),
            })
        } else {
            Err("Invalid credentials".to_string())
        }
    }

    async fn save_user_preferences(&self, user_id: u32, preferences: &UserPreferences) -> Result<(), String> {
        println!("DB: Saving preferences for user {}: {:?}", user_id, preferences);

        // Simulate database operation
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Ok(())
    }

    async fn sync_data(&self, changes: &[DataChange]) -> Result<Vec<String>, String> {
        println!("DB: Syncing {} changes", changes.len());

        // Simulate sync operation
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        Ok(changes.iter().map(|c| c.id.clone()).collect())
    }
}

#[derive(Debug, Clone)]
struct NotificationService {
    email_enabled: bool,
    push_enabled: bool,
    webhook_url: Option<String>,
}

impl NotificationService {
    async fn send_notification(&self, user: &User, notification: &Notification) -> Result<(), String> {
        if !user.preferences.notifications_enabled {
            println!("Notifications disabled for user {}", user.username);
            return Ok(());
        }

        println!(
            "NOTIFY: Sending {:?} to {} - {}",
            notification.level, user.email, notification.message
        );

        // Simulate notification sending
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CacheService {
    default_ttl: u64,
    max_size: usize,
}

impl CacheService {
    fn get(&self, key: &str, cache: &HashMap<String, CachedItem>) -> Option<String> {
        if let Some(item) = cache.get(key) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now - item.timestamp < item.ttl_seconds {
                println!("CACHE HIT: {}", key);
                Some(item.data.clone())
            } else {
                println!("CACHE EXPIRED: {}", key);
                None
            }
        } else {
            println!("CACHE MISS: {}", key);
            None
        }
    }

    fn set(&self, key: String, data: String) -> CachedItem {
        println!("CACHE SET: {} -> {}", key, data);
        CachedItem {
            data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ttl_seconds: self.default_ttl,
        }
    }
}

// ============================================================================
// Events - Domain-driven design
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    // User domain events
    UserLoginAttempt { username: String, password: String },
    UserLoginSuccess { user: User, session_token: String },
    UserLoginFailed { error: String },
    UserLogout,
    UserUpdatePreferences { preferences: UserPreferences },

    // App domain events
    ShowNotification { notification: Notification },
    DismissNotification { notification_id: String },
    ClearAllNotifications,
    OperationStarted { operation_id: String, description: String },
    OperationProgress { operation_id: String, progress: f32 },
    OperationCompleted { operation_id: String },
    OperationFailed { operation_id: String, error: String },

    // Data domain events
    DataChangeRequested { change: DataChange },
    DataSyncRequested,
    DataSyncCompleted { synced_ids: Vec<String> },
    DataSyncFailed { error: String },
    CacheDataRequested { key: String },
    CacheDataFound { key: String, data: String },
    CacheDataNotFound { key: String },

    // Error handling
    ErrorOccurred { error: String, context: String },
    ErrorRecovered { context: String },
}

// ============================================================================
// Effects - External world interactions
// ============================================================================

#[derive(Debug, Clone)]
enum AppEffect {
    // Authentication effects
    AuthenticateUser { username: String, password: String },
    GenerateSessionToken { user_id: u32 },
    InvalidateSession { session_token: String },

    // Persistence effects
    SaveUserPreferences { user_id: u32, preferences: UserPreferences },
    SyncDataChanges { changes: Vec<DataChange> },

    // Notification effects
    SendNotification { user_id: u32, notification: Notification },

    // Background operations
    StartBackgroundOperation { operation_id: String, task_type: String },

    // Logging and monitoring
    LogEvent { level: String, message: String, context: String },
    RecordMetric { metric_name: String, value: f64 },
}

// Define our complete storage type
type CompleteStorage = Storage<
    DataState,
    Storage<AppState, Storage<UserState, EmptyStorage>>
>;

type ResourceStorage = Storage<
    CacheService,
    Storage<NotificationService, Storage<DatabaseService, EmptyStorage>>
>;

// ============================================================================
// Update Functions - Domain logic
// ============================================================================

fn update_app(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, CompleteStorage>,
) -> Command<AppEvent, AppEffect> {
    match event {
        // User domain
        AppEvent::UserLoginAttempt { username, password } => {
            let user_state: &mut UserState = ctx.model_mut();
            let app_state: &mut AppState = ctx.model_mut();

            user_state.login_attempts += 1;
            app_state.is_loading = true;
            app_state.error_message = None;

            println!("Login attempt #{} for user: {}", user_state.login_attempts, username);

            Command::batch([
                Command::effect(AppEffect::AuthenticateUser { username, password }),
                Command::effect(AppEffect::LogEvent {
                    level: "INFO".to_string(),
                    message: "User login attempt".to_string(),
                    context: "authentication".to_string(),
                }),
            ])
        }

        AppEvent::UserLoginSuccess { user, session_token } => {
            let user_state: &mut UserState = ctx.model_mut();
            let app_state: &mut AppState = ctx.model_mut();

            user_state.current_user = Some(user.clone());
            user_state.session_token = Some(session_token);
            user_state.login_attempts = 0;
            app_state.is_loading = false;
            app_state.error_message = None;

            let welcome_notification = Notification {
                id: "welcome".to_string(),
                message: format!("Welcome back, {}!", user.username),
                level: NotificationLevel::Success,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            Command::batch([
                Command::event(AppEvent::ShowNotification { notification: welcome_notification }),
                Command::effect(AppEffect::LogEvent {
                    level: "INFO".to_string(),
                    message: format!("User {} logged in successfully", user.username),
                    context: "authentication".to_string(),
                }),
                Command::effect(AppEffect::RecordMetric {
                    metric_name: "user_login_success".to_string(),
                    value: 1.0,
                }),
            ])
        }

        AppEvent::UserLoginFailed { error } => {
            let user_state: &UserState = ctx.model();
            let app_state: &mut AppState = ctx.model_mut();

            app_state.is_loading = false;
            app_state.error_message = Some(error.clone());

            let error_notification = Notification {
                id: "login_error".to_string(),
                message: format!("Login failed: {}", error),
                level: NotificationLevel::Error,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            // Rate limiting - too many attempts
            if user_state.login_attempts > 3 {
                Command::event(AppEvent::ErrorOccurred {
                    error: "Too many login attempts".to_string(),
                    context: "rate_limiting".to_string(),
                })
            } else {
                Command::event(AppEvent::ShowNotification { notification: error_notification })
            }
        }

        AppEvent::UserLogout => {
            let user_state: &mut UserState = ctx.model_mut();
            let session_token = user_state.session_token.clone();

            user_state.current_user = None;
            user_state.session_token = None;
            user_state.login_attempts = 0;

            let commands = vec![
                Command::event(AppEvent::ClearAllNotifications),
                Command::effect(AppEffect::LogEvent {
                    level: "INFO".to_string(),
                    message: "User logged out".to_string(),
                    context: "authentication".to_string(),
                }),
            ];

            if let Some(token) = session_token {
                Command::batch([
                    Command::batch(commands),
                    Command::effect(AppEffect::InvalidateSession { session_token: token }),
                ])
            } else {
                Command::batch(commands)
            }
        }

        AppEvent::UserUpdatePreferences { preferences } => {
            let user_state: &mut UserState = ctx.model_mut();

            if let Some(ref mut user) = user_state.current_user {
                user.preferences = preferences.clone();

                Command::batch([
                    Command::effect(AppEffect::SaveUserPreferences {
                        user_id: user.id,
                        preferences,
                    }),
                    Command::effect(AppEffect::LogEvent {
                        level: "INFO".to_string(),
                        message: "User preferences updated".to_string(),
                        context: "user_settings".to_string(),
                    }),
                ])
            } else {
                Command::event(AppEvent::ErrorOccurred {
                    error: "Cannot update preferences - user not logged in".to_string(),
                    context: "authorization".to_string(),
                })
            }
        }

        // App state management
        AppEvent::ShowNotification { notification } => {
            let app_state: &mut AppState = ctx.model_mut();
            let user_state: &UserState = ctx.model();

            app_state.notification_queue.push(notification.clone());

            // Send notification if user is logged in
            if let Some(ref user) = user_state.current_user {
                Command::effect(AppEffect::SendNotification {
                    user_id: user.id,
                    notification,
                })
            } else {
                Command::none()
            }
        }

        AppEvent::DismissNotification { notification_id } => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.notification_queue.retain(|n| n.id != notification_id);

            Command::none()
        }

        AppEvent::ClearAllNotifications => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.notification_queue.clear();

            Command::none()
        }

        AppEvent::OperationStarted { operation_id, description } => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.active_operations.insert(
                operation_id.clone(),
                OperationStatus {
                    operation_id: operation_id.clone(),
                    status: description,
                    progress: 0.0,
                },
            );

            Command::effect(AppEffect::StartBackgroundOperation {
                operation_id,
                task_type: "generic".to_string(),
            })
        }

        AppEvent::OperationProgress { operation_id, progress } => {
            let app_state: &mut AppState = ctx.model_mut();

            if let Some(op) = app_state.active_operations.get_mut(&operation_id) {
                op.progress = progress;
                println!("Operation {} progress: {:.1}%", operation_id, progress * 100.0);
            }

            Command::none()
        }

        AppEvent::OperationCompleted { operation_id } => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.active_operations.remove(&operation_id);

            let notification = Notification {
                id: format!("op_complete_{}", operation_id),
                message: format!("Operation {} completed successfully", operation_id),
                level: NotificationLevel::Success,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            Command::event(AppEvent::ShowNotification { notification })
        }

        AppEvent::OperationFailed { operation_id, error } => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.active_operations.remove(&operation_id);

            Command::event(AppEvent::ErrorOccurred {
                error: format!("Operation {} failed: {}", operation_id, error),
                context: "background_operation".to_string(),
            })
        }

        // Data management
        AppEvent::DataChangeRequested { change } => {
            let data_state: &mut DataState = ctx.model_mut();

            data_state.pending_changes.push(change);

            // Auto-sync when we have enough pending changes
            if data_state.pending_changes.len() >= 5 {
                Command::event(AppEvent::DataSyncRequested)
            } else {
                Command::none()
            }
        }

        AppEvent::DataSyncRequested => {
            let data_state: &DataState = ctx.model();

            if !data_state.pending_changes.is_empty() {
                Command::batch([
                    Command::event(AppEvent::OperationStarted {
                        operation_id: "data_sync".to_string(),
                        description: "Synchronizing data changes".to_string(),
                    }),
                    Command::effect(AppEffect::SyncDataChanges {
                        changes: data_state.pending_changes.clone(),
                    }),
                ])
            } else {
                Command::none()
            }
        }

        AppEvent::DataSyncCompleted { synced_ids } => {
            let data_state: &mut DataState = ctx.model_mut();

            // Remove synced changes
            data_state.pending_changes.retain(|change| !synced_ids.contains(&change.id));
            data_state.last_sync = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );

            Command::event(AppEvent::OperationCompleted {
                operation_id: "data_sync".to_string(),
            })
        }

        AppEvent::DataSyncFailed { error } => {
            Command::event(AppEvent::OperationFailed {
                operation_id: "data_sync".to_string(),
                error,
            })
        }

        AppEvent::CacheDataRequested { key } => {
            let data_state: &DataState = ctx.model();

            // This would normally use the cache service, but we'll simulate it
            if let Some(item) = data_state.cache.get(&key) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if now - item.timestamp < item.ttl_seconds {
                    Command::event(AppEvent::CacheDataFound {
                        key,
                        data: item.data.clone(),
                    })
                } else {
                    Command::event(AppEvent::CacheDataNotFound { key })
                }
            } else {
                Command::event(AppEvent::CacheDataNotFound { key })
            }
        }

        AppEvent::CacheDataFound { key, data } => {
            println!("Cache hit for {}: {}", key, data);
            Command::none()
        }

        AppEvent::CacheDataNotFound { key } => {
            println!("Cache miss for {}", key);
            // Could trigger a fetch from external source
            Command::none()
        }

        // Error handling
        AppEvent::ErrorOccurred { error, context } => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.error_message = Some(error.clone());

            let error_notification = Notification {
                id: format!("error_{}", context),
                message: error.clone(),
                level: NotificationLevel::Error,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            Command::batch([
                Command::event(AppEvent::ShowNotification { notification: error_notification }),
                Command::effect(AppEffect::LogEvent {
                    level: "ERROR".to_string(),
                    message: error,
                    context,
                }),
            ])
        }

        AppEvent::ErrorRecovered { context } => {
            let app_state: &mut AppState = ctx.model_mut();

            app_state.error_message = None;

            Command::effect(AppEffect::LogEvent {
                level: "INFO".to_string(),
                message: "Error recovered".to_string(),
                context,
            })
        }
    }
}

// ============================================================================
// Effect Handlers - External integrations
// ============================================================================

async fn handle_authentication_effect(
    effect: AppEffect,
    db_service: &DatabaseService,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::AuthenticateUser { username, password } => {
            match db_service.authenticate(&username, &password).await {
                Ok(user) => {
                    let session_token = format!("session_{}_{}", user.id,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    );

                    let _ = sender.send(AppEvent::UserLoginSuccess { user, session_token });
                }
                Err(error) => {
                    let _ = sender.send(AppEvent::UserLoginFailed { error });
                }
            }
        }
        _ => {}
    }
}

async fn handle_persistence_effect(
    effect: AppEffect,
    db_service: &DatabaseService,
    sender: EventSender<AppEvent>,
) {
    match effect {
        AppEffect::SaveUserPreferences { user_id, preferences } => {
            match db_service.save_user_preferences(user_id, &preferences).await {
                Ok(()) => {
                    println!("User preferences saved successfully");
                }
                Err(error) => {
                    let _ = sender.send(AppEvent::ErrorOccurred {
                        error: format!("Failed to save preferences: {}", error),
                        context: "persistence".to_string(),
                    });
                }
            }
        }
        AppEffect::SyncDataChanges { changes } => {
            match db_service.sync_data(&changes).await {
                Ok(synced_ids) => {
                    let _ = sender.send(AppEvent::DataSyncCompleted { synced_ids });
                }
                Err(error) => {
                    let _ = sender.send(AppEvent::DataSyncFailed { error });
                }
            }
        }
        _ => {}
    }
}

async fn handle_notification_effect(
    effect: AppEffect,
    _notification_service: &NotificationService,
    _sender: EventSender<AppEvent>,
) {
    if let AppEffect::SendNotification { notification, .. } = effect {
        // In a real app, we'd need to get the user from somewhere
        // For demo purposes, we'll just print the notification
        println!(
            "NOTIFICATION: [{:?}] {}",
            notification.level, notification.message
        );
    }
}

async fn handle_background_operation(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, ResourceStorage, ExecutorStorage<TokioExecutor<AppEvent>, EmptyExecutorStorage>>,
) {
    if let AppEffect::StartBackgroundOperation { operation_id, task_type } = effect {
        let operation_id_clone = operation_id.clone();

        let ctx_clone = ctx.clone();
        // TODO: Replace with direct spawning - spawn task for background operation
        let task = async move {
            println!("Background operation {} started ({})", operation_id_clone, task_type);

            // Simulate work with progress updates
            for i in 1..=5 {
                #[cfg(feature = "tokio")]
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                let progress = (i as f32) / 5.0;
                let _ = ctx_clone.send_event(AppEvent::OperationProgress {
                    operation_id: operation_id_clone.clone(),
                    progress,
                });
            }

            // Complete the operation
            let _ = ctx_clone.send_event(AppEvent::OperationCompleted {
                operation_id: operation_id_clone,
            });
        };
        ctx.spawn(task).unwrap();
    }
}

fn handle_logging_effect(effect: AppEffect) {
    match effect {
        AppEffect::LogEvent { level, message, context } => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            println!("[{}] [{}] {} ({})", timestamp, level, message, context);
        }
        AppEffect::RecordMetric { metric_name, value } => {
            println!("METRIC: {} = {}", metric_name, value);
        }
        _ => {}
    }
}

// Main effect dispatcher
async fn handle_effects(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, ResourceStorage, ExecutorStorage<TokioExecutor<AppEvent>, EmptyExecutorStorage>>
) -> EffectResult<AppEvent> {
    let sender = EventSender(ctx.event_sender().unwrap());

    match &effect {
        AppEffect::AuthenticateUser { .. }
        | AppEffect::GenerateSessionToken { .. }
        | AppEffect::InvalidateSession { .. } => {
            let db_service: &DatabaseService = ctx.resource();
            handle_authentication_effect(effect, db_service, sender).await;
        }

        AppEffect::SaveUserPreferences { .. }
        | AppEffect::SyncDataChanges { .. } => {
            let db_service: &DatabaseService = ctx.resource();
            handle_persistence_effect(effect, db_service, sender).await;
        }

        AppEffect::SendNotification { .. } => {
            let notification_service: &NotificationService = ctx.resource();
            handle_notification_effect(effect, notification_service, sender).await;
        }

        AppEffect::StartBackgroundOperation { .. } => {
            handle_background_operation(effect, ctx).await;
        }

        AppEffect::LogEvent { .. }
        | AppEffect::RecordMetric { .. } => {
            handle_logging_effect(effect);
        }
    }
    EffectResult::None
}

// ============================================================================
// Main Application
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Real-World Application Demo ===");
    println!("Complete application with authentication, notifications, and data sync\n");

    // Build the complete system
    let (core, shell) = Syzygy::builder()
        // Application state
        .model(UserState::default())
        .model(AppState::default())
        .model(DataState::default())
        // External services
        .resource(DatabaseService {
            connection_string: "postgresql://localhost/myapp".to_string(),
            max_connections: 10,
            retry_attempts: 3,
        })
        .resource(NotificationService {
            email_enabled: true,
            push_enabled: true,
            webhook_url: Some("https://api.example.com/notify".to_string()),
        })
        .resource(CacheService {
            default_ttl: 3600, // 1 hour
            max_size: 1000,
        })
        .event_handler(update_app)
        .effect_handler(handle_effects)
        .build();
    let mut runner = Runner::new(core, shell);

    println!("Starting application workflow:\n");

    // Scenario 1: User login flow
    println!("1. User login attempt (valid credentials)");
    runner.core().send_event(AppEvent::UserLoginAttempt {
        username: "admin".to_string(),
        password: "secret".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;

    // Wait a bit for async operations
    #[cfg(feature = "tokio")]
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    runner.tick(syzygy::spawn::spawner()).await?;

    // Show current state
    let user_state: &UserState = runner.core().model();
    let app_state: &AppState = runner.core().model();
    println!("Current user: {:?}", user_state.current_user.as_ref().map(|u| &u.username));
    println!("Notifications: {}", app_state.notification_queue.len());
    println!();

    // Scenario 2: Update preferences
    println!("2. Update user preferences");
    runner.core().send_event(AppEvent::UserUpdatePreferences {
        preferences: UserPreferences {
            theme: "light".to_string(),
            language: "es".to_string(),
            notifications_enabled: true,
        },
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;

    #[cfg(feature = "tokio")]
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();

    // Scenario 3: Data operations
    println!("3. Data change operations");
    for i in 1..=6 { // This will trigger auto-sync at 5 changes
        runner.core().send_event(AppEvent::DataChangeRequested {
            change: DataChange {
                id: format!("change_{}", i),
                change_type: ChangeType::Create,
                data: format!("data_{}", i),
            },
        })?;
    }
    runner.tick(syzygy::spawn::spawner()).await?;

    // Wait for sync operation to complete
    for _ in 0..10 {
        runner.tick(syzygy::spawn::spawner()).await?;
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let app_state: &AppState = runner.core().model();
        if app_state.active_operations.is_empty() {
            break;
        }
    }
    println!();

    // Scenario 4: Error handling
    println!("4. Error handling (invalid login)");
    runner.core().send_event(AppEvent::UserLogout)?;
    runner.tick(syzygy::spawn::spawner()).await?;

    runner.core().send_event(AppEvent::UserLoginAttempt {
        username: "invalid".to_string(),
        password: "wrong".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;

    #[cfg(feature = "tokio")]
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    runner.tick(syzygy::spawn::spawner()).await?;
    println!();

    // Final state
    let user_state: &UserState = runner.core().model();
    let app_state: &AppState = runner.core().model();
    let data_state: &DataState = runner.core().model();

    println!("Final Application State:");
    println!("  User logged in: {}", user_state.current_user.is_some());
    println!("  Login attempts: {}", user_state.login_attempts);
    println!("  Active operations: {}", app_state.active_operations.len());
    println!("  Notifications: {}", app_state.notification_queue.len());
    println!("  Error message: {:?}", app_state.error_message);
    println!("  Pending data changes: {}", data_state.pending_changes.len());
    println!("  Last sync: {:?}", data_state.last_sync);

    println!("\nReal-World Application Key Points:");
    println!("✅ Domain-driven event design");
    println!("✅ Comprehensive error handling with recovery");
    println!("✅ Resource management for external services");
    println!("✅ Background operations with progress tracking");
    println!("✅ Multi-model state management");
    println!("✅ Event-driven workflows");
    println!("✅ Production-ready patterns");

    Ok(())
}
