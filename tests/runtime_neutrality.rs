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

use std::time::Duration;

use syzygy::prelude::*;

// Outcome removed; tasks now return Command directly
#[cfg(feature = "tokio")]
use syzygy::executor::TokioExecutor;

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
    model: &mut TestModel,
) -> Command<TestEvent, TestEffect> {
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
    _resources: (),
) -> syzygy::executor::Task<TestEvent, TestEffect> {
    match effect {
        TestEffect::Delay(duration) => syzygy::executor::Task::<TestEvent, TestEffect>::async_owned::<TokioExecutor, _, _>(
            move |_resources| async move {
                tokio::time::sleep(duration).await;

                Command::events(vec![TestEvent::Work])
            },
        ),
    }
}

#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn test_runtime_auto_detection() {
    let executor = TokioExecutor::multi_thread_io("runtime-auto", 2);

    let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
        .model(TestModel::default())
        .event_handler(test_update)
        .effect_handler(test_effect_handler)
        .with_async_executor(executor)
        .build();
    runner.core().event_sender().send(TestEvent::Start).unwrap();

    let (step, completed) = tokio::task::spawn_blocking(move || {
        runner
            .run_until(|core, _shell| core.model().completed)
            .expect("run_until should succeed");
        let model = runner.core().model();
        (model.step, model.completed)
    })
    .await
    .expect("spawn_blocking failed");

    assert_eq!(step, 2);
    assert!(completed);
}

#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn test_explicit_tokio_runtime() {
    let executor = TokioExecutor::current_thread_io("runtime-explicit");

    let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
        .model(TestModel::default())
        .event_handler(test_update)
        .effect_handler(test_effect_handler)
        .with_async_executor(executor)
        .build();
    runner.core().event_sender().send(TestEvent::Start).unwrap();

    while !runner.core().model().completed {
        let made_progress = runner.step().expect("step should succeed");
        if !made_progress {
            tokio::task::yield_now().await;
        }
    }

    let model = runner.core().model();
    assert_eq!(model.step, 2);
    assert!(model.completed);
}
