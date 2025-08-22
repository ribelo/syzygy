# Error Handling in Syzygy

Syzygy follows the **Error-as-Events** pattern, where all errors flow through the same event pipeline as normal application events. This ensures consistent error handling and maintains the unidirectional data flow.

## Core Principle: ALL Errors Are Events

In Syzygy applications, there are no exceptions or Result types propagating up through the system. Instead, when something goes wrong, it becomes an event that flows back to the Core for handling.

```rust
// ❌ DON'T do this - breaking the unified pipeline
fn update(&self, event: Event, model: &mut Model) -> Result<Command<Event, Effect>, Error> {
    // This breaks the error-as-events pattern
}

// ✅ DO this - errors are events
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::ProcessData { data } => {
            if data.is_empty() {
                // Error becomes an event
                return Command::event(Event::ValidationError {
                    message: "Data cannot be empty".to_string()
                });
            }
            // Success path
            model.data = data;
            Command::effect(Effect::SaveData { data: model.data.clone() })
        }
        Event::ValidationError { message } => {
            // Handle error event like any other
            model.error_message = Some(message);
            Command::none()
        }
    }
}
```

## Error Handling in Effect Handlers

Effect handlers are where most errors originate (network failures, database errors, etc.). The handler should catch all errors and convert them to events:

```rust
fn handle_effect(effect: MyEffect, ctx: EffectContext<MyEvent>) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        match effect {
            MyEffect::FetchUserData { user_id } => {
                // Wrap fallible operations in error handling
                match fetch_user_from_api(user_id).await {
                    Ok(user_data) => {
                        // Success - send success event
                        let _ = ctx.send_event(MyEvent::UserDataLoaded {
                            user_id,
                            data: user_data,
                        });
                    }
                    Err(api_error) => {
                        // Error - convert to error event
                        let _ = ctx.send_event(MyEvent::ApiError {
                            message: format!("Failed to fetch user {}: {}", user_id, api_error),
                            retry_possible: api_error.is_retryable(),
                        });
                    }
                }
            }
            
            MyEffect::SaveToDatabase { table, data } => {
                match database.save(&table, &data).await {
                    Ok(_) => {
                        let _ = ctx.send_event(MyEvent::DataSaved { table });
                    }
                    Err(db_error) => {
                        let _ = ctx.send_event(MyEvent::DatabaseError {
                            message: format!("Failed to save to {}: {}", table, db_error),
                            table,
                        });
                    }
                }
            }
        }
    })
}
```

## Error Event Patterns

### Basic Error Events

Define error events that carry enough context for the application to respond appropriately:

```rust
#[derive(Debug, Clone)]
enum AppEvent {
    // Normal events
    UserLoginRequested { username: String },
    UserDataLoaded { user_id: u32, data: UserData },
    
    // Error events with context
    ValidationError { field: String, message: String },
    NetworkError { operation: String, message: String, retry_after: Option<Duration> },
    AuthenticationFailed { username: String, reason: String },
    DatabaseError { operation: String, message: String },
    
    // Recovery events  
    RetryOperation { operation_id: String },
    FallbackActivated { original_operation: String },
}
```

### Handling Timeouts

The Shell handles effect timeouts at the infrastructure level, but your effect handlers should also implement their own timeout logic for finer control:

```rust
MyEffect::FetchWithTimeout { url, timeout_ms } => {
    let timeout_duration = Duration::from_millis(timeout_ms);
    
    match tokio::time::timeout(timeout_duration, reqwest::get(&url)).await {
        Ok(Ok(response)) => {
            // Success
            let _ = ctx.send_event(MyEvent::DataFetched {
                url: url.clone(),
                data: response.text().await.unwrap_or_default(),
            });
        }
        Ok(Err(http_error)) => {
            // HTTP error
            let _ = ctx.send_event(MyEvent::NetworkError {
                operation: format!("GET {}", url),
                message: http_error.to_string(),
                retry_after: Some(Duration::from_secs(5)),
            });
        }
        Err(_timeout_error) => {
            // Timeout error
            let _ = ctx.send_event(MyEvent::NetworkError {
                operation: format!("GET {}", url),
                message: format!("Request timed out after {}ms", timeout_ms),
                retry_after: Some(Duration::from_secs(10)),
            });
        }
    }
}
```

### Recovery and Retry Patterns

Handle errors by triggering recovery actions through new commands:

```rust
fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
    match event {
        Event::NetworkError { operation, message, retry_after } => {
            model.last_error = Some(message.clone());
            
            if model.retry_count < 3 {
                model.retry_count += 1;
                
                // Schedule retry
                Command::sequence([
                    Effect::LogError { 
                        message: format!("Operation failed: {}, retrying in {:?}", message, retry_after)
                    },
                    Effect::DelayedRetry { 
                        operation: operation.clone(),
                        delay: retry_after.unwrap_or(Duration::from_secs(5))
                    },
                ])
            } else {
                // Max retries exceeded - switch to fallback
                model.retry_count = 0;
                Command::batch([
                    Command::event(Event::FallbackActivated { original_operation: operation }),
                    Command::effect(Effect::ShowUserError { 
                        message: "Unable to complete operation, using cached data".to_string()
                    }),
                ])
            }
        }
        
        Event::DatabaseError { operation, message } => {
            // Some errors aren't retryable
            model.last_error = Some(message.clone());
            model.is_offline_mode = true;
            
            Command::batch([
                Command::effect(Effect::LogError { message }),
                Command::effect(Effect::EnableOfflineMode),
                Command::event(Event::OfflineModeActivated { reason: "Database unavailable".to_string() }),
            ])
        }
    }
}
```

## Future Effects and Error Handling

When using inline future effects (the `Future` variant), handle errors within the closure:

```rust
// ✅ Good - handle errors within the future effect
AppEffect::future(|ctx| Box::pin(async move {
    match risky_async_operation().await {
        Ok(result) => {
            let _ = ctx.send_event(AppEvent::OperationSuccess { result });
        }
        Err(error) => {
            let _ = ctx.send_event(AppEvent::OperationFailed { 
                message: error.to_string() 
            });
        }
    }
}))

// ❌ Avoid - unhandled errors can panic or get lost
AppEffect::future(|ctx| Box::pin(async move {
    let result = risky_async_operation().await.unwrap(); // Can panic!
    let _ = ctx.send_event(AppEvent::OperationSuccess { result });
}))
```

## Error State Management

Keep error information in your model for UI display and recovery logic:

```rust
#[derive(Debug, Default)]
struct AppModel {
    // Normal state
    user_data: Option<UserData>,
    is_loading: bool,
    
    // Error state
    current_error: Option<String>,
    retry_count: u32,
    is_offline_mode: bool,
    
    // Error history (for debugging/logging)
    error_history: Vec<ErrorRecord>,
}

#[derive(Debug, Clone)]
struct ErrorRecord {
    timestamp: SystemTime,
    error_type: String,
    message: String,
    context: HashMap<String, String>,
}
```

## Best Practices

1. **Never Panic** - Always convert errors to events instead of panicking
2. **Provide Context** - Include enough information in error events for proper handling
3. **Plan for Recovery** - Design error events with recovery actions in mind
4. **Log Appropriately** - Use structured logging in effect handlers, not just error events
5. **Test Error Paths** - Write tests that trigger error conditions and verify correct event flow
6. **User Experience** - Consider what the user should see/do when errors occur

## Shell Timeout Handling

The Shell provides infrastructure-level timeout handling through configuration:

```rust
let shell_config = ShellConfig {
    effect_timeout: Some(Duration::from_secs(30)), // Global timeout
    on_timeout_callback: Some(Arc::new(|| {
        eprintln!("Effect timed out - check your effect handlers for infinite loops");
    })),
    ..Default::default()
};

let shell = Shell::with_config(shell_config);
```

The timeout callback is for infrastructure monitoring, not application logic. Application-level timeout handling should happen in your effect handlers as shown above.

## Example: Complete Error Handling Flow

```rust
#[derive(Debug, Clone)]
enum AppEvent {
    StartLogin { username: String, password: String },
    LoginSuccess { user: User },
    LoginFailed { username: String, reason: LoginError },
    RetryLogin { username: String },
    ShowError { message: String },
}

#[derive(Debug, Clone)]  
enum LoginError {
    InvalidCredentials,
    NetworkTimeout,
    ServiceUnavailable,
    TooManyAttempts,
}

#[derive(Debug)]
enum AppEffect {
    AuthenticateUser { username: String, password: String },
    LogError { message: String },
    ShowUserMessage { message: String },
    DelayRetry { duration: Duration },
}

fn handle_login_effect(effect: AppEffect, ctx: EffectContext<AppEvent>) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        match effect {
            AppEffect::AuthenticateUser { username, password } => {
                // Timeout wrapper
                let auth_future = authenticate_with_server(&username, &password);
                
                match tokio::time::timeout(Duration::from_secs(10), auth_future).await {
                    Ok(Ok(user)) => {
                        let _ = ctx.send_event(AppEvent::LoginSuccess { user });
                    }
                    Ok(Err(auth_error)) => {
                        let login_error = match auth_error {
                            AuthError::InvalidCredentials => LoginError::InvalidCredentials,
                            AuthError::ServiceDown => LoginError::ServiceUnavailable,
                            AuthError::RateLimited => LoginError::TooManyAttempts,
                        };
                        let _ = ctx.send_event(AppEvent::LoginFailed { 
                            username, 
                            reason: login_error 
                        });
                    }
                    Err(_timeout) => {
                        let _ = ctx.send_event(AppEvent::LoginFailed { 
                            username, 
                            reason: LoginError::NetworkTimeout 
                        });
                    }
                }
            }
            // ... handle other effects
        }
    })
}

fn update(&self, event: AppEvent, model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::LoginFailed { username, reason } => {
            match reason {
                LoginError::NetworkTimeout if model.retry_count < 3 => {
                    model.retry_count += 1;
                    Command::batch([
                        Command::effect(AppEffect::ShowUserMessage { 
                            message: "Network timeout, retrying...".to_string() 
                        }),
                        Command::effect(AppEffect::DelayRetry { 
                            duration: Duration::from_secs(2) 
                        }),
                        // After delay, will trigger retry
                    ])
                }
                LoginError::InvalidCredentials => {
                    model.retry_count = 0;
                    Command::effect(AppEffect::ShowUserMessage { 
                        message: "Invalid username or password".to_string() 
                    })
                }
                _ => {
                    model.retry_count = 0;
                    Command::effect(AppEffect::ShowUserMessage { 
                        message: format!("Login failed: {:?}", reason) 
                    })
                }
            }
        }
        // ... handle other events
    }
}
```

This approach ensures that all errors flow through the same event pipeline, maintaining consistency and enabling sophisticated error recovery strategies.
