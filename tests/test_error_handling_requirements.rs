//! Tests for Error Handling Requirements (SYZ-014 to SYZ-015)

use syzygy::prelude::*;
use syzygy::resource::NoResources;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
    events_processed: Vec<String>,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
    ErrorOccurred(String),
}

#[derive(Debug, Clone)]
enum TestTask {
    FailingTask,
}

impl Task<TestEvent, NoResources> for TestTask {
    async fn execute(&self, _ctx: TaskContext<TestEvent, NoResources>) {
        match self {
            TestTask::FailingTask => {
                panic!("Task intentionally failed for testing panic recovery");
            }
        }
    }
}

// SYZ-014: Panic Recovery
#[tokio::test]
async fn test_panic_recovery() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    if value == 999 {
                        // This task will panic
                        Dispatch::tasks_only(vec![TestTask::FailingTask])
                    } else {
                        Dispatch::empty()
                    }
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    // Normal processing should work
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1);
    
    // Panic in task should not crash system
    handle.dispatch(TestEvent::Increment(999)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1000);
    
    // System should continue processing other events
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1002);
}

// SYZ-015: Event Error Handling
#[test]
fn test_event_error_handling() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    if value < 0 {
                        // Handle error by emitting error event instead of failing
                        Dispatch::events_only(vec![TestEvent::ErrorOccurred("Negative increment".to_string())])
                    } else {
                        state.counter += value;
                        Dispatch::empty()
                    }
                }
                TestEvent::ErrorOccurred(msg) => {
                    // Graceful error handling
                    state.events_processed.push(format!("error: {}", msg));
                    Dispatch::empty()
                }
            }
        })
        .build();
    
    // Normal case
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 5);
    
    // Error case - should recover gracefully
    handle.dispatch(TestEvent::Increment(-1)).unwrap();
    syzygy.process_events();
    
    // Counter unchanged, but error event processed
    assert_eq!(syzygy.state().counter, 5);
    assert!(syzygy.state().events_processed.contains(&"error: Negative increment".to_string()));
    
    // Pipeline continues
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 7);
}