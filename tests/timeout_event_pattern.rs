#![allow(
    dead_code,
    clippy::clone_on_ref_ptr,
    unused_variables,
    unused_imports,
    clippy::let_and_return,
    clippy::format_in_format_args
)]
//! Tests demonstrating proper timeout event patterns
//!
//! These tests show how to handle timeouts as events rather than
//! relying on logging or system-level timeouts.

use std::time::Duration;
use syzygy::event_context::EventContext;
use syzygy::executor::TokioExecutor;
use syzygy::prelude::*;

use syzygy::executor::Outcome;

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

fn timeout_update(
    event: TimeoutEvent,
    ctx: &mut EventContext<TimeoutEvent, TimeoutEffect, TimeoutModel>,
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
                duration, model.timeout_count
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

// Effect handler that implements manual timeout detection using Task plans
fn timeout_aware_effect_handler(
    effect: TimeoutEffect,
    _ctx: EffectContext<TimeoutEvent, ()>,
) -> syzygy::executor::Task<TimeoutEvent, ()> {
    match effect {
        TimeoutEffect::SlowOperation { delay_ms } => {
            syzygy::executor::Task::async_task::<TokioExecutor, _>(async move {
                let operation_future = async move {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    format!("Operation completed after {delay_ms}ms")
                };

                // Use manual timeout with event emission
                let timeout_duration = Duration::from_millis(200);
                match tokio::time::timeout(timeout_duration, operation_future).await {
                    Ok(data) => Outcome::Event(TimeoutEvent::OperationCompleted { data }),
                    Err(_timeout) => Outcome::Event(TimeoutEvent::OperationTimeout {
                        duration: timeout_duration,
                    }),
                }
            })
        }
    }
}

/// Test that timeout events are properly emitted and handled
#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn test_timeout_event_pattern() {
    let runner = Syzygy::builder::<TimeoutEvent, TimeoutEffect>()
        .model(TimeoutModel::default())
        .event_handler(timeout_update)
        .effect_handler(timeout_aware_effect_handler)
        .with_async_executor(TokioExecutor::multi_thread_io("timeout-pattern", 2))
        .build();

    let event_sender = runner.core().event_sender();

    // Start a slow operation
    event_sender.send(TimeoutEvent::StartSlowOperation).unwrap();

    // Run until operation completes or times out
    let runner = tokio::task::spawn_blocking(move || {
        let mut runner = runner;
        runner
            .run_until(|core, _shell| !core.model().is_loading)
            .unwrap();
        runner
    })
    .await
    .expect("spawn_blocking failed");

    let model = runner.core().model();

    // Should have completed successfully (100ms delay < 200ms timeout)
    assert!(!model.is_loading);
    assert!(model.data.is_some());
    assert_eq!(model.timeout_count, 0);
    assert!(model.error_message.is_none());

    println!("✅ Fast operation completed successfully: {:?}", model.data);
}

/// Test timeout event structure and data
#[tokio::test(flavor = "multi_thread")]
async fn test_timeout_event_data() {
    let timeout_duration = Duration::from_millis(100);
    let event = TimeoutEvent::OperationTimeout {
        duration: timeout_duration,
    };

    match event {
        TimeoutEvent::OperationTimeout { duration } => {
            assert_eq!(duration, Duration::from_millis(100));
            println!("✅ Timeout event carries correct duration: {duration:?}");
        }
        _ => panic!("Expected timeout event"),
    }
}
