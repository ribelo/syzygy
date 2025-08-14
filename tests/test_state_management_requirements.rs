//! Tests for State Management Requirements (SYZ-008 to SYZ-011)

use syzygy::prelude::*;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
    user_name: Option<String>,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
    SetUser(String),
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

// SYZ-008 & SYZ-009: Model and Resource Access Control
#[test]
fn test_model_and_resource_access_control() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    // State provides mutable access
                    state.counter += value;
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    // State can be mutated in event handlers
    handle.dispatch(TestEvent::Increment(10)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 10);
}

// SYZ-010: Controlled Model Mutation
#[test]
fn test_controlled_model_mutation() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            // Mutations only allowed within event handlers
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value; // This is allowed
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    // Direct mutation outside handlers is not possible due to private access
    // syzygy.state_mut() is not available - only state() for read access
    assert_eq!(syzygy.state().counter, 0);
    
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 5);
}

// SYZ-011: Immediate Consistency
#[test]
fn test_immediate_consistency() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    // Generate another event that should see the updated state
                    if state.counter == 5 {
                        Dispatch::events_only(vec![TestEvent::SetUser("triggered".to_string())])
                    } else {
                        Dispatch::empty()
                    }
                }
                TestEvent::SetUser(name) => {
                    // This handler should see the updated counter value immediately
                    state.user_name = Some(format!("{}_{}", name, state.counter));
                    Dispatch::empty()
                }
            }
        })
        .build();
    
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    
    // Subsequent event processing should see updated state immediately
    assert_eq!(syzygy.state().user_name, Some("triggered_5".to_string()));
}