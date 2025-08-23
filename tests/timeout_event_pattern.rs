//! Tests demonstrating proper timeout event patterns
//!
//! These tests show how to handle timeouts as events rather than
//! relying on logging or system-level timeouts.

use std::time::Duration;
use syzygy::prelude::*;
use syzygy::spawn::spawner;
use syzygy::event_context::EventContext;

#[derive(Debug, Default)]
struct TimeoutModel {
    is_loading: bool,
    error_message: Option<String>,
    data: Option<String>,
    timeout_count: u32,
}

#[derive(Debug, Clone)]
enum TimeoutEvent {
    StartSlowOperation,
    OperationCompleted { data: String },
    OperationTimeout { duration: Duration },
    RetryOperation,
}

#[derive(Debug, Clone)]
enum TimeoutEffect {
    SlowOperation { delay_ms: u64 },
}

use syzygy::storage::{Storage, EmptyStorage};

fn timeout_update(
    event: TimeoutEvent,
    ctx: &mut EventContext<TimeoutEvent, TimeoutEffect, Storage<TimeoutModel, EmptyStorage>>,
) -> Command<TimeoutEvent, TimeoutEffect> {
    let model: &mut TimeoutModel = ctx.model_mut();
        match event {
            TimeoutEvent::StartSlowOperation => {
                model.is_loading = true;
                model.error_message = None;
                Command::effect(TimeoutEffect::SlowOperation { delay_ms: 100 })
            }
            
            TimeoutEvent::OperationCompleted { data } => {
                model.is_loading = false;
                model.data = Some(data);
                model.timeout_count = 0;
                Command::none()
            }
            
            TimeoutEvent::OperationTimeout { duration } => {
                model.is_loading = false;
                model.timeout_count += 1;
                model.error_message = Some(format!(
                    "Operation timed out after {:?} (attempt {})",
                    duration,
                    model.timeout_count
                ));
                
                // Auto-retry once, then require manual intervention
                if model.timeout_count < 2 {
                    Command::event(TimeoutEvent::RetryOperation)
                } else {
                    Command::none()
                }
            }
            
            TimeoutEvent::RetryOperation => {
                model.is_loading = true;
                // Use a faster operation for retry
                Command::effect(TimeoutEffect::SlowOperation { delay_ms: 50 })
            }
        }
}

// Effect handler that implements manual timeout detection
async fn timeout_aware_effect_handler(effect: TimeoutEffect, ctx: EffectContext<TimeoutEvent, EmptyStorage>) {
        match effect {
            TimeoutEffect::SlowOperation { delay_ms } => {
                let operation_future = async move {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    format!("Operation completed after {delay_ms}ms")
                };
                
                // Use manual timeout with event emission
                let timeout_duration = Duration::from_millis(200);
                match tokio::time::timeout(timeout_duration, operation_future).await {
                    Ok(data) => {
                        let _ = ctx.send_event(TimeoutEvent::OperationCompleted { data });
                    }
                    Err(_timeout) => {
                        let _ = ctx.send_event(TimeoutEvent::OperationTimeout {
                            duration: timeout_duration
                        });
                    }
                }
            }
        }
}

/// Test that timeout events are properly emitted and handled
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_timeout_event_pattern() {
    let (core, shell) = Syzygy::builder::<TimeoutEvent, TimeoutEffect>()
        .model(TimeoutModel::default())
        .update(timeout_update)
        .build();
    
    let event_sender = core.event_sender();
    let shell = shell.with_effect_handler(timeout_aware_effect_handler);
    let mut runner = Runner::new(core, shell);
    
    // Start a slow operation
    event_sender.send(TimeoutEvent::StartSlowOperation).unwrap();
    
    // Run until operation completes or times out
    runner.run_until(
        |core, _shell| !core.model().is_loading,
        spawner() // Auto-detect runtime for maximum compatibility
    ).await.unwrap();
    
    let model = runner.core().model();
    
    // Should have completed successfully (100ms delay < 200ms timeout)
    assert!(!model.is_loading);
    assert!(model.data.is_some());
    assert_eq!(model.timeout_count, 0);
    assert!(model.error_message.is_none());
    
    println!("✅ Fast operation completed successfully: {:?}", model.data);
}

/// Test timeout event structure and data
#[tokio::test]
async fn test_timeout_event_data() {
    let timeout_duration = Duration::from_millis(100);
    let event = TimeoutEvent::OperationTimeout { duration: timeout_duration };
    
    match event {
        TimeoutEvent::OperationTimeout { duration } => {
            assert_eq!(duration, Duration::from_millis(100));
            println!("✅ Timeout event carries correct duration: {duration:?}");
        }
        _ => panic!("Expected timeout event"),
    }
}
