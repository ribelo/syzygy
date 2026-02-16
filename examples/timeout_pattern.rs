//! Demonstrates modeling timeouts as explicit events.
//!
//! Run with:
//! ```bash
//! cargo run --example timeout_pattern --features examples
//! ```

use std::time::Duration;

use syzygy::executor::Task;
use syzygy::prelude::*;
use syzygy::syzygy::SyzygyConfig;
use syzygy_executor_tokio::TokioExecutor;

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
    model: &mut TimeoutModel,
) -> Command<TimeoutEvent, TimeoutEffect> {
    match event {
        TimeoutEvent::StartSlowOperation => {
            model.is_loading = true;
            model.error_message = None;
            cmd::effect(TimeoutEffect::SlowOperation { delay_ms: 150 })
        }
        TimeoutEvent::OperationCompleted { data } => {
            model.is_loading = false;
            model.data = Some(data);
            model.timeout_count = 0;
            cmd::none()
        }
        TimeoutEvent::OperationTimeout { duration } => {
            model.is_loading = false;
            model.timeout_count += 1;
            model.error_message = Some(format!(
                "Operation timed out after {:?} (attempt {})",
                duration, model.timeout_count
            ));

            if model.timeout_count < 2 {
                cmd::event(TimeoutEvent::RetryOperation)
            } else {
                cmd::none()
            }
        }
        TimeoutEvent::RetryOperation => {
            model.is_loading = true;
            cmd::effect(TimeoutEffect::SlowOperation { delay_ms: 80 })
        }
    }
}

fn timeout_effect_handler(
    effect: TimeoutEffect,
    _resources: (),
) -> Task<TimeoutEvent, TimeoutEffect> {
    match effect {
        TimeoutEffect::SlowOperation { delay_ms } => {
            Task::async_on::<TokioExecutor, _>(async move {
                let operation = async move {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    format!("Operation completed after {delay_ms}ms")
                };

                let timeout_window = Duration::from_millis(120);
                match tokio::time::timeout(timeout_window, operation).await {
                    Ok(data) => cmd::event(TimeoutEvent::OperationCompleted { data }),
                    Err(_) => cmd::event(TimeoutEvent::OperationTimeout {
                        duration: timeout_window,
                    }),
                }
            })
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<TimeoutEvent, TimeoutEffect>()
        .model(TimeoutModel::default())
        .event_handler(timeout_update)
        .effect_handler(timeout_effect_handler)
        .with_effect_channel_capacity(Some(1024))
        .with_event_channel_capacity(Some(1024))
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(0)))
        .with_async_executor(TokioExecutor::current_thread_io("timeout-pattern"))
        .build();

    app.core()
        .try_send_event(TimeoutEvent::StartSlowOperation)?;

    app.run_until(|core, _| !core.model().is_loading)?;

    let model = app.core().model();
    println!(
        "Loaded data: {:?}, timeouts: {}, error: {:?}",
        model.data, model.timeout_count, model.error_message
    );

    Ok(())
}
