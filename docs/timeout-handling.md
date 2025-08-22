# Timeout Handling in Syzygy

## Overview

Syzygy provides configurable timeouts for effect execution to prevent stuck effects from blocking your application. When an effect times out, you have several patterns available for handling the timeout condition.

## Current Behavior

When an effect times out:
1. The effect execution is cancelled
2. A timeout error is logged to stderr
3. The system continues processing other effects

## Timeout Event Patterns

Since Syzygy follows the error-as-events pattern, timeouts should also be handled as events. Here are the recommended patterns:

### Pattern 1: System-Level Timeout Events (Recommended)

Add timeout events to your application's event type:

```rust
use syzygy::prelude::*;
use std::time::Duration;

#[derive(Debug, Clone)]
enum AppEvent {
    // Your regular events
    UserAction,
    DataLoaded { data: String },
    
    // System events for error handling
    EffectTimeout { effect_type: String, duration: Duration },
    EffectPanic { effect_type: String, message: String },
    SystemError { message: String },
}

#[derive(Debug, Clone)]
enum AppEffect {
    FetchData { url: String },
    SaveData { data: String },
    LongRunningTask { id: u32 },
}

// In your effect handler, implement timeout detection
async fn handle_effect(effect: AppEffect, ctx: EffectContext<AppEvent>) {
    match effect {
        AppEffect::FetchData { url } => {
            // Implement timeout detection within the effect
            let timeout_duration = Duration::from_secs(10);
            
            let fetch_future = async {
                // Your actual fetch logic here
                tokio::time::sleep(Duration::from_secs(5)).await;
                "data".to_string()
            };
            
            match tokio::time::timeout(timeout_duration, fetch_future).await {
                Ok(data) => {
                    // Success - send success event
                    let _ = ctx.send_event(AppEvent::DataLoaded { data });
                }
                Err(_timeout) => {
                    // Timeout - send timeout event
                    let _ = ctx.send_event(AppEvent::EffectTimeout {
                        effect_type: "FetchData".to_string(),
                        duration: timeout_duration,
                    });
                }
            }
        }
        
        AppEffect::LongRunningTask { id } => {
            // Alternative: Use Shell's timeout + manual detection
            let start = std::time::Instant::now();
            
            // Your long-running work
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Check if we're getting close to Shell timeout
            let elapsed = start.elapsed();
            if elapsed > Duration::from_secs(25) { // Assuming 30s Shell timeout
                let _ = ctx.send_event(AppEvent::EffectTimeout {
                    effect_type: format!("LongRunningTask({})", id),
                    duration: elapsed,
                });
            }
        }
        
        _ => {}
    }
}

// Handle timeout events in your update function
impl App for MyApp {
    type Event = AppEvent;
    type Model = AppModel;
    type Effect = AppEffect;
    
    fn update(&self, event: AppEvent, model: &mut AppModel) -> Command<AppEvent, AppEffect> {
        match event {
            AppEvent::EffectTimeout { effect_type, duration } => {
                // Update model to reflect timeout
                model.error_message = Some(format!(
                    "Effect '{}' timed out after {:?}", 
                    effect_type, 
                    duration
                ));
                model.is_loading = false;
                
                // Optionally retry or show user notification
                Command::effect(AppEffect::ShowNotification {
                    message: "Operation timed out. Please try again.".to_string()
                })
            }
            
            AppEvent::UserAction => {
                model.is_loading = true;
                Command::effect(AppEffect::FetchData {
                    url: "https://api.example.com/data".to_string()
                })
            }
            
            _ => Command::none()
        }
    }
}
```

### Pattern 2: Effect-Specific Timeout Handling

For more granular control, handle timeouts per effect type:

```rust
#[derive(Debug, Clone)]
enum DataEvent {
    FetchStarted,
    FetchCompleted { data: String },
    FetchTimeout,
    FetchRetrying { attempt: u32 },
}

async fn handle_data_effect(effect: DataEffect, ctx: EffectContext<DataEvent>) {
    match effect {
        DataEffect::FetchWithRetry { url, max_attempts } => {
            for attempt in 1..=max_attempts {
                let _ = ctx.send_event(DataEvent::FetchRetrying { attempt });
                
                let fetch_future = fetch_data(&url);
                let timeout_result = tokio::time::timeout(
                    Duration::from_secs(5), 
                    fetch_future
                ).await;
                
                match timeout_result {
                    Ok(Ok(data)) => {
                        let _ = ctx.send_event(DataEvent::FetchCompleted { data });
                        return;
                    }
                    Ok(Err(_)) => {
                        // Network error, continue retrying
                        continue;
                    }
                    Err(_timeout) => {
                        if attempt == max_attempts {
                            let _ = ctx.send_event(DataEvent::FetchTimeout);
                            return;
                        }
                        // Continue retrying
                    }
                }
            }
        }
    }
}
```

### Pattern 3: Generic Timeout Handler

For applications that need consistent timeout handling across all effects:

```rust
use std::any::type_name;

// Generic timeout wrapper
async fn with_timeout<F, T>(
    future: F,
    timeout: Duration,
    effect_name: &str,
    ctx: &EffectContext<AppEvent>,
) -> Option<T>
where
    F: std::future::Future<Output = T>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(result) => Some(result),
        Err(_timeout) => {
            let _ = ctx.send_event(AppEvent::EffectTimeout {
                effect_type: effect_name.to_string(),
                duration: timeout,
            });
            None
        }
    }
}

// Usage in effect handler
async fn handle_effect(effect: AppEffect, ctx: EffectContext<AppEvent>) {
    match effect {
        AppEffect::FetchData { url } => {
            let result = with_timeout(
                fetch_data(&url),
                Duration::from_secs(10),
                "FetchData",
                &ctx,
            ).await;
            
            if let Some(data) = result {
                let _ = ctx.send_event(AppEvent::DataLoaded { data });
            }
            // Timeout already handled by with_timeout
        }
    }
}
```

## Shell Configuration

Configure effect timeouts in your Shell:

```rust
let shell = Shell::new()
    .with_config(ShellConfig {
        effect_timeout: Some(Duration::from_secs(30)), // Global timeout
        runtime: Time::Tokio,
    })
    .with_effect_handler(handle_effect);
```

## Best Practices

1. **Always handle timeouts as events** - Don't rely on logging alone
2. **Set appropriate timeout values** - Balance responsiveness with allowing effects to complete
3. **Implement retries for transient failures** - Network requests, file I/O, etc.
4. **Update UI state on timeout** - Clear loading indicators, show error messages
5. **Consider progressive timeouts** - Short timeout for quick feedback, longer for actual failure
6. **Monitor timeout patterns** - Frequent timeouts may indicate system issues

## Example: Complete Timeout Handling

```rust
use syzygy::prelude::*;
use std::time::Duration;

#[derive(Debug, Default)]
struct AppModel {
    is_loading: bool,
    error_message: Option<String>,
    data: Option<String>,
    timeout_count: u32,
}

#[derive(Debug, Clone)]
enum AppEvent {
    StartFetch,
    FetchCompleted { data: String },
    FetchTimeout,
    RetryFetch,
    ClearError,
}

#[derive(Debug, Clone)]
enum AppEffect {
    FetchData { url: String },
    ShowToast { message: String },
}

#[derive(Default)]
struct MyApp;

impl App for MyApp {
    type Event = AppEvent;
    type Model = AppModel;
    type Effect = AppEffect;
    
    fn update(&self, event: AppEvent, model: &mut AppModel) -> Command<AppEvent, AppEffect> {
        match event {
            AppEvent::StartFetch => {
                model.is_loading = true;
                model.error_message = None;
                Command::effect(AppEffect::FetchData {
                    url: "https://api.example.com/slow-endpoint".to_string()
                })
            }
            
            AppEvent::FetchCompleted { data } => {
                model.is_loading = false;
                model.data = Some(data);
                model.timeout_count = 0; // Reset on success
                Command::none()
            }
            
            AppEvent::FetchTimeout => {
                model.is_loading = false;
                model.timeout_count += 1;
                model.error_message = Some(format!(
                    "Request timed out (attempt {}). The server might be slow.",
                    model.timeout_count
                ));
                
                // Auto-retry up to 3 times, then require manual retry
                if model.timeout_count < 3 {
                    Command::batch([
                        Command::effect(AppEffect::ShowToast {
                            message: "Retrying...".to_string()
                        }),
                        Command::event(AppEvent::RetryFetch),
                    ])
                } else {
                    Command::effect(AppEffect::ShowToast {
                        message: "Please check your connection and try again.".to_string()
                    })
                }
            }
            
            AppEvent::RetryFetch => {
                // Delay retry to avoid overwhelming slow servers
                tokio::time::sleep(Duration::from_secs(2));
                model.is_loading = true;
                Command::effect(AppEffect::FetchData {
                    url: "https://api.example.com/slow-endpoint".to_string()
                })
            }
            
            AppEvent::ClearError => {
                model.error_message = None;
                model.timeout_count = 0;
                Command::none()
            }
        }
    }
}

async fn handle_effect(effect: AppEffect, ctx: EffectContext<AppEvent>) {
    match effect {
        AppEffect::FetchData { url } => {
            let fetch_future = async {
                // Simulate slow API call
                tokio::time::sleep(Duration::from_secs(8)).await;
                "API response data".to_string()
            };
            
            // Use manual timeout to send proper events
            match tokio::time::timeout(Duration::from_secs(5), fetch_future).await {
                Ok(data) => {
                    let _ = ctx.send_event(AppEvent::FetchCompleted { data });
                }
                Err(_timeout) => {
                    let _ = ctx.send_event(AppEvent::FetchTimeout);
                }
            }
        }
        
        AppEffect::ShowToast { message } => {
            println!("🍞 Toast: {}", message);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (core, shell) = Syzygy::builder::<MyApp>()
        .app(MyApp)
        .model(AppModel::default())
        .build();
    
    let shell = shell
        .with_config(ShellConfig {
            effect_timeout: Some(Duration::from_secs(10)), // Backup timeout
            runtime: Time::Tokio,
        })
        .with_effect_handler(handle_effect);
    
    let mut runner = Runner::new(core, shell);
    
    // Start the fetch
    runner.core_mut().send_event(AppEvent::StartFetch)?;
    
    // Run until we have data or give up
    runner.run_until(
        |core, _shell| core.model().data.is_some() || core.model().timeout_count >= 3,
        |future| { tokio::spawn(future); }
    ).await?;
    
    println!("Final state: {:?}", runner.core().model());
    Ok(())
}
```

This pattern ensures that timeouts are handled gracefully and users receive appropriate feedback when operations take too long.
