#![cfg(feature = "shell")]
//! Tests for Shell synchronous methods: `drain_with`, `dispatch_command`, etc.
//! These tests use the Runner API for convenience.

use syzygy::executor::{InlineAsync, PanicTaskKind, Task};
use syzygy::prelude::*;
use syzygy_executor_single::SingleThreadExecutor;
use syzygy_executor_tokio::TokioExecutor;

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
            cmd::event(TestEvent::Pong)
        }
        TestEvent::Pong => {
            model.count += 1;
            cmd::effect(TestEffect::Log)
        }
    }
}

#[cfg(feature = "shell")]
mod tokio_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use syzygy::error::ShellError;
    use syzygy::syzygy::SyzygyConfig;
    use tokio_util::sync::CancellationToken;

    fn create_test_runner() -> Runner<TestEvent, TestEffect, TestModel> {
        Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .with_async_executor(InlineAsync::new())
            .build()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn effect_queue_respects_capacity_limits() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .with_effect_channel_capacity(Some(1))
            .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
            .build();

        let shell = runner.shell_mut();
        assert_eq!(shell.effect_channel_capacity(), Some(1));
        shell
            .dispatch_command(Command::effect(TestEffect::Log))
            .expect("first effect should fit in queue");

        let error = shell
            .dispatch_command(Command::effect(TestEffect::Log))
            .expect_err("second effect should exceed capacity");

        match error {
            ShellError::EffectQueueFull { capacity } => assert_eq!(capacity, 1),
            other => panic!("expected EffectQueueFull, got {other:?}"),
        }
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
        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

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
            runner
                .core_mut()
                .try_send_event(TestEvent::Ping)
                .expect("event channel should be open");
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
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        let err = runner
            .step()
            .expect_err("step should fail when required executor is missing");

        match err {
            ShellError::TaskSpawnFailed(message) => {
                assert!(
                    message.contains("Missing async executor"),
                    "error should mention missing async executor, got: {message:?}"
                );
                assert!(
                    message.contains("TokioExecutor"),
                    "error should include executor type name, got: {message:?}"
                );
            }
            other => panic!("Expected TaskSpawnFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn await_idle_waits_for_async_work() {
        #[derive(Debug, Clone)]
        enum CliEvent {
            Start,
            Done,
        }

        #[derive(Debug, Default)]
        struct CliModel {
            completed: usize,
        }

        #[derive(Debug, Clone)]
        enum CliEffect {
            Work,
        }

        fn cli_update(event: CliEvent, model: &mut CliModel) -> Command<CliEvent, CliEffect> {
            match event {
                CliEvent::Start => Command::effect(CliEffect::Work),
                CliEvent::Done => {
                    model.completed += 1;
                    Command::none()
                }
            }
        }

        fn cli_effects(effect: CliEffect, _: ()) -> Task<CliEvent, CliEffect> {
            match effect {
                CliEffect::Work => Task::async_on::<TokioExecutor, _>(async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Command::event(CliEvent::Done)
                }),
            }
        }

        let mut runner = Syzygy::builder::<CliEvent, CliEffect>()
            .model(CliModel::default())
            .event_handler(cli_update)
            .effect_handler(cli_effects)
            .with_async_executor(TokioExecutor::current_thread_io("shell-cli"))
            .build();

        runner
            .core_mut()
            .try_send_event(CliEvent::Start)
            .expect("event channel should be open");

        runner
            .await_idle(Duration::from_secs(1))
            .expect("await_idle should return once work completes");

        assert_eq!(runner.core().model().completed, 1);
        assert!(runner.shell().is_idle());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dispatch_and_await_processes_command() {
        #[derive(Debug, Clone)]
        enum CliEvent {
            Done,
        }

        #[derive(Debug, Default)]
        struct CliModel {
            hits: usize,
        }

        #[derive(Debug, Clone)]
        enum CliEffect {
            Work,
        }

        fn cli_update(event: CliEvent, model: &mut CliModel) -> Command<CliEvent, CliEffect> {
            match event {
                CliEvent::Done => {
                    model.hits += 1;
                    Command::none()
                }
            }
        }

        fn cli_effects(effect: CliEffect, _: ()) -> Task<CliEvent, CliEffect> {
            match effect {
                CliEffect::Work => Task::async_on::<TokioExecutor, _>(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Command::event(CliEvent::Done)
                }),
            }
        }

        let mut runner = Syzygy::builder::<CliEvent, CliEffect>()
            .model(CliModel::default())
            .event_handler(cli_update)
            .effect_handler(cli_effects)
            .with_async_executor(TokioExecutor::current_thread_io("shell-cli-dispatch"))
            .build();

        runner
            .dispatch_and_await(Command::effect(CliEffect::Work), Duration::from_secs(1))
            .expect("dispatch_and_await should process command");

        assert_eq!(runner.core().model().hits, 1);
        assert!(runner.shell().is_idle());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_max_limits_iterations() {
        let mut runner = create_test_runner();
        for _ in 0..3 {
            runner
                .core_mut()
                .try_send_event(TestEvent::Ping)
                .expect("event channel should be open");
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
        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

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
    async fn drain_until_idle_waits_for_shell() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|effect: TestEffect, _| match effect {
                TestEffect::Log => {
                    Task::<TestEvent, TestEffect>::async_on::<TokioExecutor, _>(async move {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        cmd::none()
                    })
                }
                TestEffect::Work(_) => Task::none(),
            })
            .with_async_executor(TokioExecutor::current_thread_io("drain-idle"))
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        runner
            .drain_until_idle(Duration::from_millis(500))
            .expect("drain_until_idle should observe idle");

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        let err = runner
            .drain_until_idle(Duration::from_millis(1))
            .expect_err("expected timeout when idle not reached in time");
        assert!(matches!(err, ShellError::Timeout { .. }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn async_current_with_cancel_dispatches_cancel_command() {
        #[derive(Debug, Default)]
        struct CancelModel {
            cancelled: usize,
        }

        #[derive(Debug, Clone)]
        enum CancelEvent {
            Start,
            Cancelled,
        }

        #[derive(Debug, Clone)]
        enum CancelEffect {
            Run,
        }

        let token = CancellationToken::new();
        let token_for_runner = token.clone();

        let mut runner =
            Syzygy::builder::<CancelEvent, CancelEffect>()
                .model(CancelModel::default())
                .with_resources(token_for_runner)
                .event_handler(|event, model| match event {
                    CancelEvent::Start => cmd::effect(CancelEffect::Run),
                    CancelEvent::Cancelled => {
                        model.cancelled += 1;
                        cmd::none()
                    }
                })
                .effect_handler(
                    |effect: CancelEffect, token: CancellationToken| match effect {
                        CancelEffect::Run => {
                            let cancel_future = token.cancelled_owned();
                            Task::<CancelEvent, CancelEffect>::async_on_with_cancel::<
                                TokioExecutor,
                                _,
                                _,
                            >(
                                async move {
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                    cmd::event(CancelEvent::Cancelled)
                                },
                                cancel_future,
                                cmd::event(CancelEvent::Cancelled),
                            )
                        }
                    },
                )
                .with_async_executor(TokioExecutor::current_thread_io("cancel-runner"))
                .with_async_executor(InlineAsync::new())
                .build();

        runner
            .core_mut()
            .try_send_event(CancelEvent::Start)
            .expect("event channel should be open");

        token.cancel();

        runner
            .drain_until_idle(Duration::from_secs(1))
            .expect("should observe idle after cancellation");

        assert_eq!(runner.core().model().cancelled, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn panic_handler_turns_panics_into_events() {
        #[derive(Debug, Default)]
        struct PanicModel {
            messages: Vec<String>,
        }

        #[derive(Debug, Clone)]
        enum PanicEvent {
            Trigger,
            Notified(String),
        }

        #[derive(Debug, Clone)]
        enum PanicEffect {
            Explode,
        }

        let observed_details = Arc::new(Mutex::new(None));
        let observed_for_handler = Arc::clone(&observed_details);

        let mut runner = Syzygy::builder::<PanicEvent, PanicEffect>()
            .model(PanicModel::default())
            .event_handler(|event, model| match event {
                PanicEvent::Trigger => cmd::effect(PanicEffect::Explode),
                PanicEvent::Notified(message) => {
                    model.messages.push(message);
                    cmd::none()
                }
            })
            .effect_handler(|effect: PanicEffect, _| match effect {
                PanicEffect::Explode => {
                    Task::<PanicEvent, PanicEffect>::async_on::<TokioExecutor, _>(async move {
                        panic!("boom!");
                        #[allow(unreachable_code)]
                        {
                            cmd::none()
                        }
                    })
                }
            })
            .with_panic_handler(move |details, message| {
                *observed_for_handler.lock().unwrap() = Some(details);
                cmd::event(PanicEvent::Notified(message))
            })
            .with_async_executor(TokioExecutor::current_thread_io("panic-hook"))
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(PanicEvent::Trigger)
            .expect("event channel should be open");

        runner
            .drain_until_idle(Duration::from_secs(1))
            .expect("panic handler should drain to idle");

        let model = runner.core().model();
        assert_eq!(model.messages.len(), 1);
        assert!(model.messages[0].contains("boom"));

        let details = observed_details
            .lock()
            .unwrap()
            .expect("panic details should be recorded");
        assert_eq!(details.kind, PanicTaskKind::Async);
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

                Task::<TestEvent, TestEffect>::async_on::<TokioExecutor, _>(async move {
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
            .with_async_executor(TokioExecutor::current_thread_io("batch-sequential"))
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

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
            .with_async_executor(TokioExecutor::multi_thread_io("parallel-overlap", 2))
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

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

                Task::<TestEvent, TestEffect>::blocking_with_resource_on::<
                    SingleThreadExecutor<()>,
                    (),
                    _,
                >(move |_| {
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
            .with_resource_blocking_executor(SingleThreadExecutor::new())
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");
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
            .with_async_executor(InlineAsync::new())
            .build();

        // Set idle sleep to a long duration
        let config = SyzygyConfig {
            idle_sleep: Duration::from_millis(200),
        };
        runner.set_config(config);

        // Send an event so there's work available
        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

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
            .with_async_executor(InlineAsync::new())
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
