//! Tests for Event Processing Requirements (SYZ-004 to SYZ-007)

use std::time::Duration;
use syzygy::prelude::*;

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

impl<E, R> Task<E, R> for TestTask 
where
    E: Clone + Send,
    R: Send,
{
    async fn execute(&self, _ctx: TaskContext<E, R>) {
        // Simple task implementation for testing
    }
}

// SYZ-004: Non-Blocking Event Dispatch
#[test]
fn test_non_blocking_event_dispatch() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();

    let start = std::time::Instant::now();
    
    // Dispatch should not block
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    handle.dispatch(TestEvent::Increment(3)).unwrap();
    
    let dispatch_time = start.elapsed();
    
    // Dispatch should complete very quickly (non-blocking)
    assert!(dispatch_time < Duration::from_millis(1));
    
    // Events should be queued for processing
    assert!(syzygy.has_pending_events());
    
    // Process events
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 6);
}

// SYZ-005: Sequential Event Processing
#[test]
fn test_sequential_event_processing() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.events_processed.push(format!("increment_{}", value));
                    state.counter += value;
                    Dispatch::empty()
                }
                TestEvent::SetUser(name) => {
                    state.events_processed.push(format!("set_user_{}", name));
                    state.user_name = Some(name);
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();

    // Queue multiple events
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    handle.dispatch(TestEvent::SetUser("Alice".to_string())).unwrap();
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    
    // Process all events
    syzygy.process_events();
    
    // Events should be processed in FIFO order
    assert_eq!(syzygy.state().events_processed, vec![
        "increment_1",
        "set_user_Alice", 
        "increment_2"
    ]);
    assert_eq!(syzygy.state().counter, 3);
    assert_eq!(syzygy.state().user_name, Some("Alice".to_string()));
}

// SYZ-006: Pure Event Handlers
#[test]
fn test_pure_event_handlers() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            // Handler should be pure - no side effects, only state changes and returned effects
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    // Return new events and tasks (pure functional style)
                    Dispatch::new(
                        vec![TestEvent::ProcessingComplete],
                        vec![TestTask::LogEvent(format!("Incremented by {}", value))]
                    )
                }
                _ => Dispatch::empty(),
            }
        })
        .build();

    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    
    // Handler should have modified state and generated new effects
    assert_eq!(syzygy.state().counter, 5);
    // Additional events should be queued
    assert!(syzygy.has_pending_events());
}

// SYZ-007: Dispatch Builder Pattern
#[test]
fn test_eventresult_builder_pattern() {
    let result = Dispatch::new(
        vec![TestEvent::Increment(1), TestEvent::SetUser("Bob".to_string())],
        vec![TestTask::LogEvent("test".to_string())]
    );
    
    // Should support multiple events and tasks
    assert!(!result.events.is_empty());
    assert!(!result.tasks.is_empty());
    
    // Can also build with specific builders
    let events_result: Dispatch<TestEvent, TestTask> = Dispatch::events_only(vec![TestEvent::Increment(1)]);
    assert!(!events_result.events.is_empty());
    assert!(events_result.tasks.is_empty());
    
    let tasks_result: Dispatch<TestEvent, TestTask> = Dispatch::tasks_only(vec![TestTask::LogEvent("test".to_string())]);
    assert!(tasks_result.events.is_empty());
    assert!(!tasks_result.tasks.is_empty());
    
    // Can also build empty results
    let empty_result: Dispatch<TestEvent, TestTask> = Dispatch::empty();
    assert!(empty_result.events.is_empty());
    assert!(empty_result.tasks.is_empty());
}