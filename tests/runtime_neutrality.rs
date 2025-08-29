#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args)]
//! Integration tests for runtime neutrality
//!
//! These tests verify that Syzygy works correctly with different async runtimes.
//! We test each runtime explicitly to ensure compatibility, though tokio is the
//! primary/recommended runtime for production use.

use std::time::Duration;
use syzygy::prelude::*;
use syzygy::spawn::spawner;

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

use syzygy::storage::{EmptyStorage, Storage};

fn test_update(
    event: TestEvent,
    ctx: &mut EventContext<TestEvent, TestEffect, Storage<TestModel, EmptyStorage>>,
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

async fn test_effect_handler(effect: TestEffect, ctx: EffectContext<TestEvent, EmptyStorage>) {
    match effect {
        TestEffect::Delay(duration) => {
            // Use runtime-neutral sleep
            #[cfg(feature = "tokio")]
            tokio::time::sleep(duration).await;

            #[cfg(all(feature = "smol", not(feature = "tokio")))]
            smol::Timer::after(duration).await;

            #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
            async_std::task::sleep(duration).await;

            // Send event back using EffectContext
            let _ = ctx.send_event(TestEvent::Work);
        }
    }
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_tokio_runtime() {
    let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
        .model(TestModel::default())
        .event_handler(test_update)
        .effect_handler(test_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    let event_sender = runner.core().event_sender();

    // Start the test sequence
    event_sender.send(TestEvent::Start).unwrap();

    // Run until completed - using spawn adapter
    runner
        .run_until(
            |core, _shell| core.model().completed,
            syzygy::spawn::TokioSpawn,
        )
        .await
        .unwrap();

    // Verify the sequence completed
    assert_eq!(runner.core().model().step, 2);
    assert!(runner.core().model().completed);
}

#[cfg(all(feature = "smol", not(feature = "tokio")))]
#[test]
fn test_smol_runtime() {
    smol::block_on(async {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(test_effect_handler)
            .build();

        let mut runner = Runner::new(core, shell);
        let event_sender = runner.core().event_sender();

        // Start the test sequence
        event_sender.send(TestEvent::Start).unwrap();

        // Run until completed - using spawn adapter
        runner
            .run_until(
                |core, _shell| core.model().completed,
                syzygy::spawn::SmolSpawn,
            )
            .await
            .unwrap();

        // Verify the sequence completed
        assert_eq!(runner.core().model().step, 2);
        assert!(runner.core().model().completed);
    });
}

#[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
#[test]
fn test_async_std_runtime() {
    async_std::task::block_on(async {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(test_effect_handler)
            .build();

        let mut runner = Runner::new(core, shell);
        let event_sender = runner.core().event_sender();

        // Start the test sequence
        event_sender.send(TestEvent::Start).unwrap();

        // Run until completed - using spawn adapter
        runner
            .run_until(
                |core, _shell| core.model().completed,
                syzygy::spawn::AsyncStdSpawn,
            )
            .await
            .unwrap();

        // Verify the sequence completed
        assert_eq!(runner.core().model().step, 2);
        assert!(runner.core().model().completed);
    });
}

/// Test auto_spawn functionality
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_auto_spawn_adapter() {
    let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
        .model(TestModel::default())
        .event_handler(test_update)
        .effect_handler(test_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    let event_sender = runner.core().event_sender();

    // Start the test sequence
    event_sender.send(TestEvent::Start).unwrap();

    // Run until completed - using auto spawn detection
    runner
        .run_until(
            |core, _shell| core.model().completed,
            spawner(), // This should automatically use tokio since it's enabled
        )
        .await
        .unwrap();

    // Verify the sequence completed
    assert_eq!(runner.core().model().step, 2);
    assert!(runner.core().model().completed);

    println!("✅ Auto-spawn adapter works correctly with tokio runtime");
}

#[cfg(all(feature = "smol", not(feature = "tokio")))]
#[test]
fn test_auto_spawn_with_smol() {
    smol::block_on(async {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(test_effect_handler)
            .build();

        let mut runner = Runner::new(core, shell);
        let event_sender = runner.core().event_sender();

        event_sender.send(TestEvent::Start).unwrap();

        // Auto-spawn should detect and use smol
        runner
            .run_until(|core, _shell| core.model().completed, spawner())
            .await
            .unwrap();

        assert_eq!(runner.core().model().step, 2);
        assert!(runner.core().model().completed);

        println!("✅ Auto-spawn adapter works correctly with smol runtime");
    });
}

// Note: No runtime test is not needed anymore - the code now fails to compile
// when no runtime features are enabled, which is much better than runtime panics.

/// Test that timer abstractions work correctly
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_timer_abstraction() {
    use std::time::Instant;
    use syzygy::timer::time;

    // Test sleep function
    let runtime = time();
    let start = Instant::now();
    runtime.sleep(Duration::from_millis(50)).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(40)); // Allow some variance
    assert!(elapsed < Duration::from_millis(100));

    // Test timeout function
    let quick_future = Box::pin(async {});
    let result = runtime.timeout(Duration::from_secs(1), quick_future).await;
    assert!(result.is_ok());

    let slow_future = Box::pin(async {
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let result = runtime
        .timeout(Duration::from_millis(50), slow_future)
        .await;
    assert!(result.is_err());
}

/// Test that config runtime settings survive cloning
#[cfg(feature = "tokio")]
#[test]
fn test_config_runtime_preserved_after_clone() {
    use syzygy::prelude::*;

    // Test RunnerConfig cloning preserves runtime setting
    #[cfg(feature = "smol")]
    {
        let config = RunnerConfig {
            runtime: syzygy::timer::Time::Smol,
            ..RunnerConfig::default()
        };
        let cloned = config.clone();
        assert!(matches!(cloned.runtime, syzygy::timer::Time::Smol));
    }

    // Test ShellConfig cloning preserves runtime setting
    #[cfg(feature = "smol")]
    {
        let config = ShellConfig {
            runtime: syzygy::timer::Time::Smol,
            ..ShellConfig::default()
        };
        let cloned = config.clone();
        assert!(matches!(cloned.runtime, syzygy::timer::Time::Smol));
    }

    // Test with tokio since we know it's enabled in this test
    let config = RunnerConfig {
        runtime: syzygy::timer::Time::Tokio,
        ..RunnerConfig::default()
    };
    let cloned = config.clone();
    assert!(matches!(cloned.runtime, syzygy::timer::Time::Tokio));
}
