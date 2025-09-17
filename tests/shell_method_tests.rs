//! Tests for Shell synchronous methods: `drain_with`, `dispatch_command`, etc.
//! These tests use the Runner API for convenience.

use syzygy::executor::Outcome;
use syzygy::prelude::*;
use syzygy::scheduler::{Scheduler, TokioScheduler, blocking_scheduler};

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

fn test_update(
    event: TestEvent,
    ctx: &mut EventContext<TestEvent, TestEffect, TestModel>,
) -> Command<TestEvent, TestEffect> {
    let model = ctx.model_mut();
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
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn create_test_runner() -> Syzygy<TestEvent, TestEffect, TestModel, ()> {
        let registry = syzygy::executor::ExecutorRegistry::new();

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _ctx| async { Outcome::None })
            .with_executor_registry(registry)
            .build();

        runner
    }

    #[tokio::test]
    async fn shell_pending_effects_starts_at_zero() {
        let runner = create_test_runner();
        let shell = runner.shell();
        assert_eq!(shell.pending_effects(), 0, "Should start with empty queue");
    }

    #[tokio::test]
    async fn shell_handles_shutdown_state_correctly() {
        let mut runner = create_test_runner();

        // Initially not closed
        assert!(!runner.shell().is_closed());

        // Shutdown via runner
        runner.shutdown();
        assert!(runner.shell().is_closed());
    }

    #[tokio::test]
    async fn shell_config_access_works() {
        let runner = create_test_runner();
        let shell = runner.shell();

        // Should be able to access config
        let config = shell.config();
        assert!(
            config.effect_channel_capacity.is_none(),
            "Default queue should be unbounded"
        );
    }

    #[tokio::test]
    async fn shell_processes_events_and_effects_via_runner() {
        let mut runner = create_test_runner();
        let scheduler = TokioScheduler::new().expect("Should have tokio runtime");

        // Send an event that will generate another event and an effect
        runner.core_mut().send_event(TestEvent::Ping);

        // First step: process Ping -> generates Pong event (which is routed back to Core)
        let did_work1 = runner
            .step_with(scheduler.clone())
            .expect("First step should succeed");
        assert!(did_work1, "First step should process Ping event");

        // Second step: process Pong event -> generates AND processes Log effect in same step
        let did_work2 = runner
            .step_with(scheduler.clone())
            .expect("Second step should succeed");
        assert!(
            did_work2,
            "Second step should process Pong event and Log effect"
        );

        // Third step: no more work (effect was already processed in step 2)
        let did_work3 = runner
            .step_with(scheduler)
            .expect("Third step should succeed");
        assert!(!did_work3, "Third step should have no more work");
    }

    #[tokio::test]
    async fn shell_handles_multiple_events_correctly() {
        let mut runner = create_test_runner();
        let scheduler = TokioScheduler::new().expect("Should have tokio runtime");

        // Send multiple events
        for _ in 0..3 {
            runner.core_mut().send_event(TestEvent::Ping);
        }

        // Process all events and effects
        let mut total_steps = 0;
        while runner
            .step_with(scheduler.clone())
            .expect("Step should succeed")
        {
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

    #[tokio::test]
    async fn batch_effects_execute_in_sequence() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let registry = syzygy::executor::ExecutorRegistry::new();
        let order_for_handler = order.clone();
        let in_flight_for_handler = in_flight.clone();
        let max_in_flight_for_handler = max_in_flight.clone();

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _ctx| match event {
                TestEvent::Ping => Command::sequential([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _ctx| {
                let order = order_for_handler.clone();
                let in_flight = in_flight_for_handler.clone();
                let max_in_flight = max_in_flight_for_handler.clone();

                async move {
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

                    Outcome::None
                }
            })
            .with_executor_registry(registry)
            .build();

        runner.core_mut().send_event(TestEvent::Ping);
        let scheduler = TokioScheduler::new().expect("Should have tokio runtime");

        while runner
            .step_with(scheduler.clone())
            .expect("step should succeed")
        {
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

    #[tokio::test]
    async fn shell_default_queue_is_unbounded() {
        let mut runner = create_test_runner();
        let shell = runner.shell_mut();

        let command = Command::batch((0..1_500).map(|_| Command::effect(TestEffect::Log)));

        shell
            .dispatch_command(command)
            .expect("Should not hit an artificial queue limit");

        assert_eq!(shell.pending_effects(), 1_500);

        let processed = shell
            .drain_with(blocking_scheduler())
            .expect("Draining should succeed");

        assert_eq!(processed, 1_500);
        assert_eq!(shell.pending_effects(), 0);
    }

    #[derive(Clone)]
    struct SerialScheduler {
        inner: TokioScheduler,
    }

    impl SerialScheduler {
        fn new() -> Self {
            Self {
                inner: TokioScheduler::new().expect("Should have tokio runtime"),
            }
        }
    }

    impl Scheduler for SerialScheduler {
        fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
            self.inner.schedule(future);
        }

        fn allows_overlap(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn parallel_effects_overlap_on_concurrent_scheduler() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let registry = syzygy::executor::ExecutorRegistry::new();
        let order_for_handler = order.clone();
        let in_flight_for_handler = in_flight.clone();
        let max_in_flight_for_handler = max_in_flight.clone();

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _ctx| match event {
                TestEvent::Ping => Command::parallel([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _ctx| {
                let order = order_for_handler.clone();
                let in_flight = in_flight_for_handler.clone();
                let max_in_flight = max_in_flight_for_handler.clone();

                async move {
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

                    Outcome::None
                }
            })
            .with_executor_registry(registry)
            .build();

        runner.core_mut().send_event(TestEvent::Ping);
        let scheduler = TokioScheduler::new().expect("Should have tokio runtime");

        while runner
            .step_with(scheduler.clone())
            .expect("step should succeed")
        {
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
            "Parallel effects should overlap on concurrent scheduler",
        );
    }

    #[tokio::test]
    async fn parallel_effects_respect_non_overlapping_scheduler() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let registry = syzygy::executor::ExecutorRegistry::new();
        let order_for_handler = order.clone();
        let in_flight_for_handler = in_flight.clone();
        let max_in_flight_for_handler = max_in_flight.clone();

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _ctx| match event {
                TestEvent::Ping => Command::parallel([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _ctx| {
                let order = order_for_handler.clone();
                let in_flight = in_flight_for_handler.clone();
                let max_in_flight = max_in_flight_for_handler.clone();

                async move {
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

                    Outcome::None
                }
            })
            .with_executor_registry(registry)
            .build();

        runner.core_mut().send_event(TestEvent::Ping);
        let scheduler = SerialScheduler::new();

        while runner
            .step_with(scheduler.clone())
            .expect("step should succeed")
        {
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
            "No overlapping work should occur without scheduler support",
        );
    }
}
