//! Tests for Shell synchronous methods: `drain_with`, `dispatch_command`, etc.
//! These tests use the Runner API for convenience.

use syzygy::executor::{InlineAsync, Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum TestEvent {
    Ping,
    Pong,
}

#[derive(Debug, Clone)]
enum TestEffect {
    Log,
    Work(u8),
}

#[derive(Debug, Default)]
struct TestModel {
    count: i32,
}

fn test_update(event: TestEvent, model: &mut TestModel) -> Command<TestEvent, TestEffect> {
    match event {
        TestEvent::Ping => {
            model.count += 1;
            Command::event(TestEvent::Pong)
        }
        TestEvent::Pong => {
            model.count += 1;
            Command::effect(TestEffect::Log)
        }
    }
}

#[cfg(feature = "tokio")]
mod tokio_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use syzygy::error::ShellError;

    fn create_test_runner() -> Syzygy<TestEvent, TestEffect, TestModel> {
        Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .with_async_executor(InlineAsync::<TestEvent>::new())
            .build()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_pending_effects_starts_at_zero() {
        let runner = create_test_runner();
        let shell = runner.shell();
        assert_eq!(shell.pending_effects(), 0, "Should start with empty queue");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_handles_shutdown_state_correctly() {
        let mut runner = create_test_runner();

        // Initially not closed
        assert!(!runner.shell().is_closed());

        // Shutdown via runner
        runner.shutdown();
        assert!(runner.shell().is_closed());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_config_access_works() {
        let runner = create_test_runner();
        let shell = runner.shell();

        // Should be able to access config
        assert!(
            shell.effect_channel_capacity().is_none(),
            "Default queue should be unbounded"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_processes_events_and_effects_via_runner() {
        let mut runner = create_test_runner();

        // Send an event that will generate another event and an effect
        runner.core_mut().send_event(TestEvent::Ping);

        // First step: process Ping -> generates Pong event (which is routed back to Core)
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(did_work1, "First step should process Ping event");

        // Second step: process Pong event -> generates AND processes Log effect in same step
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(
            did_work2,
            "Second step should process Pong event and Log effect"
        );

        // Third step: no more work (effect was already processed in step 2)
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(!did_work3, "Third step should have no more work");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_handles_multiple_events_correctly() {
        let mut runner = create_test_runner();

        // Send multiple events
        for _ in 0..3 {
            runner.core_mut().send_event(TestEvent::Ping);
        }

        // Process all events and effects
        let mut total_steps = 0;
        while runner.step().expect("Step should succeed") {
            total_steps += 1;
        }

        // Core processes ALL events in queue at once:
        // Step 1: Process all 3 Ping events -> generates 3 Pong events
        // Step 2: Process all 3 Pong events -> generates and processes 3 Log effects
        assert_eq!(
            total_steps, 2,
            "Should process exactly 2 steps for 3 Ping events (batch processing)"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_reports_missing_async_executor() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::effect(TestEffect::Work(99)),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(|effect: TestEffect, _resources| match effect {
                TestEffect::Work(_) => {
                    Task::<TestEvent, TestEffect>::async_on::<TokioExecutor, _>(async move {
                        Command::none()
                    })
                }
                TestEffect::Log => Task::<TestEvent, TestEffect>::none(),
            })
            .with_async_executor(InlineAsync::<TestEvent>::new())
            .build();

        runner.core_mut().send_event(TestEvent::Ping);

        let err = runner
            .step()
            .expect_err("step should fail when required executor is missing");

        match err {
            ShellError::TaskSpawnFailed(message) => {
                assert!(
                    message.contains("Missing async executor"),
                    "error should mention missing async executor, got: {message:?}"
                );
            }
            other => panic!("Expected TaskSpawnFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_max_limits_iterations() {
        let mut runner = create_test_runner();
        for _ in 0..3 {
            runner.core_mut().send_event(TestEvent::Ping);
        }

        let steps = runner.drain_max(1).expect("drain_max should succeed");
        assert_eq!(steps, 1, "Should only perform a single step");

        // Some work remains; another drain should make progress.
        let more_steps = runner.drain_max(10).expect("second drain should succeed");
        assert!(more_steps >= 1, "Expected additional work to be processed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_until_completes_or_times_out() {
        let mut runner = create_test_runner();
        runner.core_mut().send_event(TestEvent::Ping);

        runner
            .drain_until(|model| model.count >= 2, Duration::from_secs(1))
            .expect("drain_until should complete");
        assert!(runner.core().model().count >= 2);

        let timeout = runner
            .drain_until(|model| model.count > 100, Duration::from_millis(10))
            .expect_err("expected timeout");
        assert!(matches!(timeout, ShellError::Timeout { .. }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn batch_effects_execute_in_sequence() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let order_for_handler = Arc::clone(&order);
        let in_flight_for_handler = Arc::clone(&in_flight);
        let max_in_flight_for_handler = Arc::clone(&max_in_flight);

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::sequential([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _resources| {
                let order = Arc::clone(&order_for_handler);
                let in_flight = Arc::clone(&in_flight_for_handler);
                let max_in_flight = Arc::clone(&max_in_flight_for_handler);

                Task::<TestEvent, TestEffect>::async_on::<InlineAsync<TestEvent>, _>(async move {
                    if let TestEffect::Work(id) = effect {
                        let active = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(active, Ordering::SeqCst);

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("start-{id}"));
                        }

                        std::thread::sleep(Duration::from_millis(5));

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("end-{id}"));
                        }

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    }

                    Command::none()
                })
            })
            .with_async_executor(InlineAsync::<TestEvent>::new())
            .build();

        runner.core_mut().send_event(TestEvent::Ping);

        while runner.step().expect("step should succeed") {
            tokio::task::yield_now().await;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let log = order.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "start-1".to_string(),
                "end-1".to_string(),
                "start-2".to_string(),
                "end-2".to_string(),
                "start-3".to_string(),
                "end-3".to_string(),
            ],
            "Batch effects should run strictly sequentially",
        );
        assert_eq!(max_in_flight.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_default_queue_is_unbounded() {
        let mut runner = create_test_runner();
        let shell = runner.shell_mut();

        let command = Command::batch((0..1_500).map(|_| Command::effect(TestEffect::Log)));

        shell
            .dispatch_command(command)
            .expect("Should not hit an artificial queue limit");

        assert_eq!(shell.pending_effects(), 1_500);

        let processed = shell.drain().expect("Draining should succeed");

        assert_eq!(processed, 1_500);
        assert_eq!(shell.pending_effects(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parallel_effects_overlap_on_concurrent_executor() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let order_for_handler = Arc::clone(&order);
        let in_flight_for_handler = Arc::clone(&in_flight);
        let max_in_flight_for_handler = Arc::clone(&max_in_flight);

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::parallel([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _resources| {
                let order = Arc::clone(&order_for_handler);
                let in_flight = Arc::clone(&in_flight_for_handler);
                let max_in_flight = Arc::clone(&max_in_flight_for_handler);

                Task::<TestEvent, TestEffect>::async_on::<TokioExecutor, _>(async move {
                    if let TestEffect::Work(id) = effect {
                        let active = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(active, Ordering::SeqCst);

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("start-{id}"));
                        }

                        tokio::time::sleep(Duration::from_millis(10)).await;

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("end-{id}"));
                        }

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    }

                    Command::none()
                })
            })
            .with_async_executor(TokioExecutor::current_thread_io("parallel-overlap"))
            .build();

        runner.core_mut().send_event(TestEvent::Ping);

        while runner.step().expect("step should succeed") {
            tokio::task::yield_now().await;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let log = order.lock().unwrap().clone();
        let mut starts: Vec<_> = log
            .iter()
            .filter(|entry| entry.starts_with("start-"))
            .cloned()
            .collect();
        starts.sort();
        assert_eq!(
            starts,
            vec![
                "start-1".to_string(),
                "start-2".to_string(),
                "start-3".to_string(),
            ],
            "All effects should have started",
        );

        let mut ends: Vec<_> = log
            .iter()
            .filter(|entry| entry.starts_with("end-"))
            .cloned()
            .collect();
        ends.sort();
        assert_eq!(
            ends,
            vec![
                "end-1".to_string(),
                "end-2".to_string(),
                "end-3".to_string(),
            ],
            "All effects should have completed",
        );

        assert!(
            max_in_flight.load(Ordering::SeqCst) >= 2,
            "Parallel effects should overlap on concurrent executor",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parallel_effects_respect_non_overlapping_executor() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let order_for_handler = Arc::clone(&order);
        let in_flight_for_handler = Arc::clone(&in_flight);
        let max_in_flight_for_handler = Arc::clone(&max_in_flight);

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::parallel([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _resources| {
                let order = Arc::clone(&order_for_handler);
                let in_flight = Arc::clone(&in_flight_for_handler);
                let max_in_flight = Arc::clone(&max_in_flight_for_handler);

                Task::<TestEvent, TestEffect>::async_on::<InlineAsync<TestEvent>, _>(async move {
                    if let TestEffect::Work(id) = effect {
                        let active = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(active, Ordering::SeqCst);

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("start-{id}"));
                        }

                        tokio::time::sleep(Duration::from_millis(5)).await;

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("end-{id}"));
                        }

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    }

                    Command::none()
                })
            })
            .with_async_executor(InlineAsync::<TestEvent>::new())
            .build();

        runner.core_mut().send_event(TestEvent::Ping);
        while runner.step().expect("step should succeed") {
            tokio::task::yield_now().await;
        }

        tokio::time::sleep(Duration::from_millis(30)).await;

        let log = order.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "start-1".to_string(),
                "end-1".to_string(),
                "start-2".to_string(),
                "end-2".to_string(),
                "start-3".to_string(),
                "end-3".to_string(),
            ],
            "Parallel effects should fall back to sequential order when overlap is disabled",
        );

        assert_eq!(
            max_in_flight.load(Ordering::SeqCst),
            1,
            "No overlapping work should occur when executor disallows overlap",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wait_for_work_returns_immediately_when_work_available() {
        use std::time::Duration;

        // Create a runner with non-zero idle sleep
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .with_async_executor(InlineAsync::<TestEvent>::new())
            .build();

        // Set idle sleep to a long duration
        let config = SyzygyConfig {
            idle_sleep: Duration::from_millis(200),
        };
        runner.set_config(config);

        // Send an event so there's work available
        runner.core_mut().send_event(TestEvent::Ping);

        // Verify work is pending before wait_for_work
        assert!(
            runner.core().has_pending_events(),
            "Should have pending work before wait_for_work"
        );

        // wait_for_work should return immediately since there's pending work
        runner.wait_for_work();

        // Verify work is still pending after wait_for_work (it doesn't process, just waits)
        assert!(
            runner.core().has_pending_events(),
            "Should still have pending work after wait_for_work"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wait_for_work_yields_when_idle_sleep_zero() {
        use std::time::Duration;

        // Create a runner with zero idle sleep
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::none())
            .with_async_executor(InlineAsync::<TestEvent>::new())
            .build();

        // Set idle sleep to zero
        let config = SyzygyConfig {
            idle_sleep: Duration::from_millis(0),
        };
        runner.set_config(config);

        // Verify no work is pending
        assert!(
            !runner.core().has_pending_events(),
            "Should have no pending work"
        );
        assert_eq!(
            runner.shell().pending_effects(),
            0,
            "Should have no pending effects"
        );

        // wait_for_work should yield immediately when idle_sleep is zero
        runner.wait_for_work();

        // State should be unchanged - no work was added or processed
        assert!(
            !runner.core().has_pending_events(),
            "Should still have no pending work"
        );
        assert_eq!(
            runner.shell().pending_effects(),
            0,
            "Should still have no pending effects"
        );
    }
}
