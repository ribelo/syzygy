//! Tests for Shell synchronous methods: drain_with(), enqueue_command(), etc.
//! These tests use the Runner API since enqueue_command is an internal method.

use syzygy::prelude::*;
use syzygy::scheduler::TokioScheduler;

#[derive(Debug, Clone)]
enum TestEvent {
    Ping,
    Pong,
}

#[derive(Debug, Clone)]
enum TestEffect {
    Log,
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

    fn create_test_runner() -> Runner<TestEvent, TestEffect, TestModel, ()> {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _ctx| {
                // Return None instead of empty events to properly indicate no new events
                syzygy::executor::Task::none()
            })
            .build();

        Runner::new(core, shell)
    }

    #[tokio::test]
    async fn shell_pending_effects_starts_at_zero() {
        let runner = create_test_runner();
        assert_eq!(
            runner.shell().pending_effects(),
            0,
            "Should start with empty queue"
        );
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

        // Should be able to access config
        let config = runner.shell().config();
        // Config should have default values
        assert!(
            config.effect_timeout.is_some(),
            "Should have default effect timeout"
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
}
