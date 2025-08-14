//! Tests for Type Safety Requirements (SYZ-016 to SYZ-017)

use std::thread;
use syzygy::prelude::*;

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

// SYZ-016: Compile-Time Type Safety
#[test]
fn test_compile_time_type_safety() {
    // This test verifies that the type system prevents errors at compile time
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            // All types are resolved at compile time
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value; // Type-safe access
                    Dispatch::empty() // Type-safe return
                }
            }
        })
        .build();
    
    // Type-safe event dispatch
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    
    // The following would not compile if uncommented:
    // handle.dispatch("invalid_event").unwrap(); // Wrong type
    // let wrong_type: i32 = syzygy.state(); // Wrong return type
    
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1);
}

// SYZ-017: Thread Safety Requirements
#[test]
fn test_thread_safety_requirements() {
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
    
    let handle_clone = handle.clone();
    
    // Handle can be sent across threads (Send bound satisfied)
    let thread_handle = thread::spawn(move || {
        handle_clone.dispatch(TestEvent::Increment(10)).unwrap();
    });
    
    thread_handle.join().unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 10);
}