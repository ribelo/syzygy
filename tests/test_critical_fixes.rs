//! Tests for the critical fixes: panic recovery, event feedback loop, and task executor

use std::time::Duration;
use tokio::time::sleep;
use syzygy::prelude::*;
use syzygy::resource::NoResources;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
    events_processed: Vec<String>,
    error_count: i32,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
    ProcessingComplete,
    ValidationFailed { error: String },
    CascadeEvent(i32),
}

#[derive(Debug, Clone)]
enum TestTask {
    LogEvent(String),
    AsyncIncrement(i32),
    FailingTask,
}

impl Task<TestEvent, NoResources> for TestTask {
    async fn execute(&self, ctx: TaskContext<TestEvent, NoResources>) {
        match self {
            TestTask::LogEvent(_msg) => {
                // Simulate logging - in real implementation this would log
                let _ = ctx.dispatch(TestEvent::ProcessingComplete);
            }
            TestTask::AsyncIncrement(value) => {
                sleep(Duration::from_millis(10)).await;
                let _ = ctx.dispatch(TestEvent::Increment(*value));
            }
            TestTask::FailingTask => {
                panic!("Task intentionally failed for testing panic recovery");
            }
        }
    }
}

// Test 1: Event Handler Panic Recovery
#[test]
fn test_event_handler_panic_recovery() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    if value == 999 {
                        panic!("Handler intentionally panicked for testing");
                    }
                    state.counter += value;
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    // Normal processing should work
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1);
    
    // Panic in event handler should not crash system
    handle.dispatch(TestEvent::Increment(999)).unwrap();
    syzygy.process_events();
    // Counter should still be 1 (panic prevented increment)
    assert_eq!(syzygy.state().counter, 1);
    
    // System should continue processing other events
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 3);
}

// Test 2: Event Feedback Loop
#[test]
fn test_event_feedback_loop() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    // Generate cascade event
                    if value == 1 {
                        Dispatch::events_only(vec![TestEvent::CascadeEvent(5)])
                    } else {
                        Dispatch::empty()
                    }
                }
                TestEvent::CascadeEvent(value) => {
                    state.counter += value;
                    Dispatch::empty()
                }
                TestEvent::ValidationFailed { error } => {
                    state.events_processed.push(format!("error: {}", error));
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    // Dispatch initial event that should trigger cascade
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    syzygy.process_events();
    
    // Should have processed both the initial event and the cascade
    assert_eq!(syzygy.state().counter, 6); // 1 + 5
}

// Test 3: Error Events (errors as events pattern)
#[test]
fn test_error_events_pattern() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    if value < 0 {
                        // Handle error by emitting error event instead of failing
                        Dispatch::events_only(vec![TestEvent::ValidationFailed { 
                            error: "Negative increment".to_string() 
                        }])
                    } else {
                        state.counter += value;
                        Dispatch::empty()
                    }
                }
                TestEvent::ValidationFailed { error } => {
                    // Graceful error handling
                    state.error_count += 1;
                    state.events_processed.push(format!("error: {}", error));
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    // Normal case
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 5);
    assert_eq!(syzygy.state().error_count, 0);
    
    // Error case - should recover gracefully
    handle.dispatch(TestEvent::Increment(-1)).unwrap();
    syzygy.process_events();
    
    // Counter should remain unchanged, but error should be recorded
    assert_eq!(syzygy.state().counter, 5);
    assert_eq!(syzygy.state().error_count, 1);
    assert!(syzygy.state().events_processed.contains(&"error: Negative increment".to_string()));
    
    // System should continue working
    handle.dispatch(TestEvent::Increment(3)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 8);
}

// Test 4: TaskExecutor Basic Functionality
#[cfg(feature = "async")]
#[tokio::test]
async fn test_task_executor_basic() {
    let (mut syzygy, handle, executor) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    // Generate task that will dispatch event back
                    Dispatch::tasks_only(vec![TestTask::AsyncIncrement(value)])
                }
                TestEvent::ProcessingComplete => {
                    state.events_processed.push("complete".to_string());
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build_with_executor(NoResources::default());
    
    // Start the executor
    let executor_handle = executor.start();
    
    // Dispatch event that generates task
    handle.dispatch(TestEvent::Increment(3)).unwrap();
    syzygy.process_events();
    
    // Initial state should be updated
    assert_eq!(syzygy.state().counter, 3);
    
    // Wait for async task to complete and dispatch back
    sleep(Duration::from_millis(50)).await;
    syzygy.process_events();
    
    // Async task should have dispatched another increment
    assert_eq!(syzygy.state().counter, 6);
    
    // Shutdown executor
    executor_handle.shutdown().await;
}

// Test 5: TaskExecutor Panic Recovery
#[cfg(feature = "async")]
#[tokio::test]
async fn test_task_executor_panic_recovery() {
    let (mut syzygy, handle, executor) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    if value == 999 {
                        // This task will panic
                        Dispatch::tasks_only(vec![TestTask::FailingTask])
                    } else {
                        // Normal task
                        Dispatch::tasks_only(vec![TestTask::LogEvent("increment".to_string())])
                    }
                }
                TestEvent::ProcessingComplete => {
                    state.events_processed.push("complete".to_string());
                    Dispatch::empty()
                }
                _ => Dispatch::empty(),
            }
        })
        .build_with_executor(NoResources::default());
    
    // Start the executor
    let executor_handle = executor.start();
    
    // Normal processing should work
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1);
    
    // Wait for task to complete
    sleep(Duration::from_millis(50)).await;
    syzygy.process_events();
    assert!(!syzygy.state().events_processed.is_empty());
    
    // Panic in task should not crash system
    handle.dispatch(TestEvent::Increment(999)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1000);
    
    // System should continue processing other events
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 1002);
    
    // Wait and verify system still works
    sleep(Duration::from_millis(50)).await;
    syzygy.process_events();
    
    // Shutdown executor
    executor_handle.shutdown().await;
}

// Test 6: Full Integration Test
#[cfg(feature = "async")]
#[tokio::test]
async fn test_full_integration() {
    let (mut syzygy, handle, executor) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    if value < 0 {
                        // Error as event
                        Dispatch::events_only(vec![TestEvent::ValidationFailed { 
                            error: "Negative value".to_string() 
                        }])
                    } else {
                        state.counter += value;
                        // Cascade event and task
                        if value == 5 {
                            Dispatch::new(
                                vec![TestEvent::CascadeEvent(10)],
                                vec![TestTask::LogEvent("special".to_string())]
                            )
                        } else {
                            Dispatch::tasks_only(vec![TestTask::LogEvent("normal".to_string())])
                        }
                    }
                }
                TestEvent::CascadeEvent(value) => {
                    state.counter += value;
                    Dispatch::empty()
                }
                TestEvent::ValidationFailed { error } => {
                    state.error_count += 1;
                    state.events_processed.push(format!("error: {}", error));
                    Dispatch::empty()
                }
                TestEvent::ProcessingComplete => {
                    state.events_processed.push("complete".to_string());
                    Dispatch::empty()
                }
            }
        })
        .build_with_executor(NoResources::default());
    
    let executor_handle = executor.start();
    
    // Test normal flow
    handle.dispatch(TestEvent::Increment(3)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 3);
    
    // Test error flow
    handle.dispatch(TestEvent::Increment(-1)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 3); // No change
    assert_eq!(syzygy.state().error_count, 1);
    
    // Test cascade flow
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    assert_eq!(syzygy.state().counter, 18); // 3 + 5 + 10
    
    // Wait for tasks
    sleep(Duration::from_millis(100)).await;
    syzygy.process_events();
    
    // Should have completion events from tasks
    let complete_count = syzygy.state().events_processed.iter()
        .filter(|s| *s == "complete")
        .count();
    assert!(complete_count >= 2); // At least 2 tasks completed
    
    executor_handle.shutdown().await;
}