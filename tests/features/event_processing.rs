//! Cucumber tests for Event Processing Requirements (SYZ-004 to SYZ-007)

use cucumber::{given, then, when, World};
use std::time::{Duration, Instant};
use syzygy::prelude::*;
use syzygy::resource::NoResources;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
    user_name: Option<String>,
    events_processed: Vec<String>,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
    SetUser(String),
    ProcessingComplete,
}

#[derive(Debug, Clone)]
enum TestTask {
    LogEvent(String),
}

impl Task<TestEvent, NoResources> for TestTask {
    async fn execute(&self, _ctx: TaskContext<TestEvent, NoResources>) {
        // Simple task implementation for testing
    }
}

#[derive(Debug, World)]
#[world(init = Self::new)]
struct SyzygyWorld {
    syzygy: Option<Syzygy<TestState, TestEvent, TestTask>>,
    handle: Option<SyzygyHandle<TestEvent>>,
    last_dispatch_time: Option<Duration>,
    events_queued: bool,
}

impl SyzygyWorld {
    fn new() -> Self {
        Self {
            syzygy: None,
            handle: None,
            last_dispatch_time: None,
            events_queued: false,
        }
    }

    fn create_system(&mut self) {
        let (syzygy, handle) = Syzygy::builder()
            .with_state(TestState::default())
            .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
                match event {
                    TestEvent::Increment(value) => {
                        state.events_processed.push(format!("increment_{}", value));
                        state.counter += value;
                        Dispatch::new(
                            vec![TestEvent::ProcessingComplete],
                            vec![TestTask::LogEvent(format!("Incremented by {}", value))]
                        )
                    }
                    TestEvent::SetUser(name) => {
                        state.events_processed.push(format!("set_user_{}", name));
                        state.user_name = Some(name);
                        Dispatch::empty()
                    }
                    TestEvent::ProcessingComplete => {
                        state.events_processed.push("completed".to_string());
                        Dispatch::empty()
                    }
                }
            })
            .build();
        
        self.syzygy = Some(syzygy);
        self.handle = Some(handle);
    }
}

// SYZ-004: Non-Blocking Event Dispatch
#[given("a Syzygy system is running")]
fn given_syzygy_system_running(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I dispatch multiple events through the handle")]
fn when_dispatch_multiple_events(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    let start = Instant::now();
    
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    handle.dispatch(TestEvent::Increment(3)).unwrap();
    
    world.last_dispatch_time = Some(start.elapsed());
}

#[then("the dispatch calls should not block")]
fn then_dispatch_should_not_block(world: &mut SyzygyWorld) {
    let dispatch_time = world.last_dispatch_time.unwrap();
    // Dispatch should complete very quickly (non-blocking)
    assert!(dispatch_time < Duration::from_millis(1), 
           "Dispatch took too long: {:?}", dispatch_time);
}

#[then("events should be queued for processing")]
fn then_events_queued_for_processing(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_ref().unwrap();
    assert!(syzygy.has_pending_events());
    world.events_queued = true;
}

// SYZ-005: Sequential Event Processing
#[when("I dispatch events in a specific order")]
fn when_dispatch_events_in_order(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    handle.dispatch(TestEvent::SetUser("Alice".to_string())).unwrap();
    handle.dispatch(TestEvent::Increment(2)).unwrap();
}

#[when("I process all events")]
fn when_process_all_events(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_mut().unwrap();
    syzygy.process_events();
}

#[then("events should be processed in FIFO order")]
fn then_events_processed_in_fifo_order(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_ref().unwrap();
    let expected_order = vec![
        "increment_1",
        "set_user_Alice", 
        "increment_2"
    ];
    
    assert_eq!(syzygy.state().events_processed, expected_order);
    assert_eq!(syzygy.state().counter, 3);
    assert_eq!(syzygy.state().user_name, Some("Alice".to_string()));
}

// SYZ-006: Pure Event Handlers
#[when("I dispatch an event that generates new events and tasks")]
fn when_dispatch_event_generating_effects(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    handle.dispatch(TestEvent::Increment(5)).unwrap();
}

#[then("the handler should return new effects as data")]
fn then_handler_returns_effects_as_data(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_mut().unwrap();
    syzygy.process_events();
    
    // Handler should have modified state and generated new effects
    assert_eq!(syzygy.state().counter, 5);
    // Additional events should be queued (ProcessingComplete)
    assert!(syzygy.state().events_processed.contains(&"completed".to_string()));
}

// SYZ-007: Dispatch Builder Pattern
#[given("I want to create Dispatch instances")]
fn given_want_to_create_eventresult(world: &mut SyzygyWorld) {
    // Setup for testing Dispatch builder
    world.create_system();
}

#[when("I use the Dispatch builder methods")]
fn when_use_eventresult_builder(world: &mut SyzygyWorld) {
    // Test various Dispatch builder patterns
    let _full_result = Dispatch::new(
        vec![TestEvent::Increment(1), TestEvent::SetUser("Bob".to_string())],
        vec![TestTask::LogEvent("test".to_string())]
    );
    
    let _events_only: Dispatch<TestEvent, TestTask> = Dispatch::events_only(vec![TestEvent::Increment(1)]);
    let _tasks_only: Dispatch<TestEvent, TestTask> = Dispatch::tasks_only(vec![TestTask::LogEvent("test".to_string())]);
    let _empty: Dispatch<TestEvent, TestTask> = Dispatch::empty();
}

#[then("I should be able to create results with zero or many events and tasks")]
fn then_can_create_various_eventresults(world: &mut SyzygyWorld) {
    // Test the different Dispatch creation patterns
    let full_result = Dispatch::new(
        vec![TestEvent::Increment(1), TestEvent::SetUser("Bob".to_string())],
        vec![TestTask::LogEvent("test".to_string())]
    );
    assert!(!full_result.events.is_empty());
    assert!(!full_result.tasks.is_empty());
    
    let events_result: Dispatch<TestEvent, TestTask> = Dispatch::events_only(vec![TestEvent::Increment(1)]);
    assert!(!events_result.events.is_empty());
    assert!(events_result.tasks.is_empty());
    
    let tasks_result: Dispatch<TestEvent, TestTask> = Dispatch::tasks_only(vec![TestTask::LogEvent("test".to_string())]);
    assert!(tasks_result.events.is_empty());
    assert!(!tasks_result.tasks.is_empty());
    
    let empty_result: Dispatch<TestEvent, TestTask> = Dispatch::empty();
    assert!(empty_result.events.is_empty());
    assert!(empty_result.tasks.is_empty());
}

#[tokio::main]
async fn main() {
    SyzygyWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("features/syzygy-event-processing.feature")
        .await;
}