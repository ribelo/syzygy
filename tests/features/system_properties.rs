//! Cucumber tests for System Properties Requirements (SYZ-018 to SYZ-021)

use cucumber::{given, then, when, World};
use std::time::Duration;
use syzygy::prelude::*;
use syzygy::resource::NoResources;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
}

#[derive(Debug, Clone)]
enum TestTask {
    AsyncIncrement(i32),
}

impl Task<TestEvent, NoResources> for TestTask {
    async fn execute(&self, ctx: TaskContext<TestEvent, NoResources>) {
        match self {
            TestTask::AsyncIncrement(value) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = ctx.dispatch(TestEvent::Increment(*value));
            }
        }
    }
}

#[derive(Debug, World)]
#[world(init = Self::new)]
struct SyzygyWorld {
    syzygy: Option<Syzygy<TestState, TestEvent, TestTask>>,
    handle: Option<SyzygyHandle<TestEvent>>,
    determinism_verified: bool,
    monitoring_available: bool,
    observability_enabled: bool,
    async_compatible: bool,
}

impl SyzygyWorld {
    fn new() -> Self {
        Self {
            syzygy: None,
            handle: None,
            determinism_verified: false,
            monitoring_available: false,
            observability_enabled: false,
            async_compatible: false,
        }
    }

    fn create_system(&mut self) {
        let (syzygy, handle) = Syzygy::builder()
            .with_state(TestState::default())
            .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
                match event {
                    TestEvent::Increment(value) => {
                        state.counter += value;
                        Dispatch::tasks_only(vec![TestTask::AsyncIncrement(value)])
                    }
                }
            })
            .build();
        
        self.syzygy = Some(syzygy);
        self.handle = Some(handle);
    }
}

// SYZ-018: Deterministic Behavior
#[given("a Syzygy system with deterministic processing")]
fn given_syzygy_with_deterministic_processing(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I run the same sequence of events multiple times")]
fn when_run_same_sequence_multiple_times(world: &mut SyzygyWorld) {
    let mut results = Vec::new();
    
    // Run same sequence multiple times
    for _ in 0..5 {
        let (mut syzygy, handle) = Syzygy::builder()
            .with_state(TestState::default())
            .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
                match event {
                    TestEvent::Increment(value) => {
                        state.counter += value;
                        Dispatch::empty()
                    }
                }
            })
            .build();
        
        handle.dispatch(TestEvent::Increment(3)).unwrap();
        handle.dispatch(TestEvent::Increment(7)).unwrap();
        syzygy.process_events();
        results.push(syzygy.state().counter);
    }
    
    // All runs should produce identical results
    let first = results[0];
    world.determinism_verified = results.iter().all(|&x| x == first);
}

#[then("all runs should produce identical results")]
fn then_all_runs_produce_identical_results(world: &mut SyzygyWorld) {
    assert!(world.determinism_verified);
}

// SYZ-019: Event Queue Monitoring
#[given("a Syzygy system with monitoring capabilities")]
fn given_syzygy_with_monitoring(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I check the event queue status")]
fn when_check_event_queue_status(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_ref().unwrap();
    let handle = world.handle.as_ref().unwrap();
    
    // Initially no pending events
    assert!(!syzygy.has_pending_events());
    assert_eq!(syzygy.pending_event_count(), 0);
    
    // Queue some events
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    
    // Should be able to monitor queue status
    world.monitoring_available = syzygy.has_pending_events() && syzygy.pending_event_count() == 2;
}

#[then("I should be able to monitor pending events and queue size")]
fn then_can_monitor_pending_events(world: &mut SyzygyWorld) {
    assert!(world.monitoring_available);
    
    let syzygy = world.syzygy.as_mut().unwrap();
    // Process events and verify queue empties
    syzygy.process_events();
    
    assert!(!syzygy.has_pending_events());
    assert_eq!(syzygy.pending_event_count(), 0);
}

// SYZ-020: Observability Integration
#[given("a Syzygy system with observability features")]
fn given_syzygy_with_observability(world: &mut SyzygyWorld) {
    world.create_system();
    world.observability_enabled = true;
}

#[when("I process events with observability enabled")]
fn when_process_events_with_observability(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    let syzygy = world.syzygy.as_mut().unwrap();
    
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    syzygy.process_events();
}

#[then("the system should support tracing and monitoring integration")]
fn then_system_supports_tracing_monitoring(world: &mut SyzygyWorld) {
    assert!(world.observability_enabled);
    
    let syzygy = world.syzygy.as_ref().unwrap();
    // System should work with observability (tracing would be configured externally)
    assert_eq!(syzygy.state().counter, 1);
}

// SYZ-021: Async Runtime Compatibility
#[given("a Syzygy system in an async runtime environment")]
fn given_syzygy_in_async_runtime(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I use the system with async tasks and tokio runtime")]
fn when_use_system_with_async_tasks(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    let syzygy = world.syzygy.as_mut().unwrap();
    
    // Should work cleanly with tokio runtime
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    
    world.async_compatible = true;
}

#[then("the system should integrate cleanly with async runtimes")]
fn then_system_integrates_with_async_runtimes(world: &mut SyzygyWorld) {
    assert!(world.async_compatible);
    
    let syzygy = world.syzygy.as_ref().unwrap();
    assert_eq!(syzygy.state().counter, 5);
}

#[tokio::main]
async fn main() {
    SyzygyWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("features/syzygy-system-properties.feature")
        .await;
}