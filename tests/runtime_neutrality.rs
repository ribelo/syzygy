#![allow(
    dead_code,
    clippy::clone_on_ref_ptr,
    unused_variables,
    unused_imports,
    clippy::let_and_return,
    clippy::format_in_format_args
)]
//! Minimal runtime neutrality test
//!
//! Verifies Syzygy works with the primary runtime (tokio) and auto-detection.
//! Extended runtime testing is handled by spawn.rs unit tests.

use std::sync::Arc;
use std::time::Duration;

use syzygy::prelude::*;

use futures::FutureExt;
use syzygy::executor::{AsyncExecutor, Outcome, TokioExecutor};

#[derive(Debug, Clone)]
enum TestEvent {
    Start,
    Work,
    Complete,
}

#[derive(Debug, Default)]
struct TestModel {
    step: u32,
    completed: bool,
}

#[derive(Debug, Clone)]
enum TestEffect {
    Delay(Duration),
}

fn test_update(
    event: TestEvent,
    ctx: &mut EventContext<TestEvent, TestEffect, TestModel>,
) -> Command<TestEvent, TestEffect> {
    let model: &mut TestModel = ctx.model_mut();
    match event {
        TestEvent::Start => {
            model.step = 1;
            Command::effect(TestEffect::Delay(Duration::from_millis(10)))
        }
        TestEvent::Work => {
            model.step = 2;
            Command::event(TestEvent::Complete)
        }
        TestEvent::Complete => {
            model.completed = true;
            Command::none()
        }
    }
}

fn test_effect_handler(
    effect: TestEffect,
    _ctx: EffectContext<TestEvent, ()>,
) -> syzygy::executor::Task<TestEvent, ()> {
    match effect {
        TestEffect::Delay(duration) => {
            syzygy::executor::Task::async_task::<TokioExecutor, _>(async move {
                tokio::time::sleep(duration).await;

                Outcome::Events(vec![TestEvent::Work])
            })
        }
    }
}

#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn test_runtime_auto_detection() {
    let executor = TokioExecutor::multi_thread_io("runtime-auto", 2);

    let runner = Syzygy::builder::<TestEvent, TestEffect>()
        .model(TestModel::default())
        .event_handler(test_update)
        .effect_handler(test_effect_handler)
        .with_async_executor(executor)
        .build();
    let event_sender = runner.core().event_sender();

    // Start the test sequence
    event_sender.send(TestEvent::Start).unwrap();

    // Run until completed - using auto-detection spawner
    let runner = tokio::task::spawn_blocking(move || {
        let mut runner = runner;
        runner
            .run_until(|core, _shell| core.model().completed)
            .unwrap();
        runner
    })
    .await
    .expect("spawn_blocking failed");

    // Verify the sequence completed
    assert_eq!(runner.core().model().step, 2);
    assert!(runner.core().model().completed);
}

#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn test_explicit_tokio_runtime() {
    let executor = TokioExecutor::multi_thread_io("runtime-explicit", 2);

    let runner = Syzygy::builder::<TestEvent, TestEffect>()
        .model(TestModel::default())
        .event_handler(test_update)
        .effect_handler(test_effect_handler)
        .with_async_executor(executor.clone())
        .build();
    let event_sender = runner.core().event_sender();

    event_sender.send(TestEvent::Start).unwrap();

    // Run with explicit tokio spawn
    let runner = tokio::task::spawn_blocking(move || {
        let mut runner = runner;
        let executor_arc: Arc<dyn AsyncExecutor<TestEvent>> = Arc::new(executor);
        runner
            .run_until_with_executor(|core, _shell| core.model().completed, executor_arc)
            .unwrap();
        runner
    })
    .await
    .expect("spawn_blocking failed");

    assert_eq!(runner.core().model().step, 2);
    assert!(runner.core().model().completed);
}
