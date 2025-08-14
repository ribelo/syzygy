//! Tests for Task Execution Requirements (SYZ-012 to SYZ-013)

use std::time::Duration;
use tokio::time::sleep;
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
    ProcessingComplete,
}

#[derive(Debug, Clone)]
enum TestTask {
    LogEvent(String),
    AsyncIncrement(i32),
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
        }
    }
}

// SYZ-012: Async Task Execution
#[tokio::test]
async fn test_async_task_execution() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    state.counter += value;
                    // Generate async task
                    Dispatch::tasks_only(vec![TestTask::AsyncIncrement(value)])
                }
                _ => Dispatch::empty(),
            }
        })
        .build();
    
    handle.dispatch(TestEvent::Increment(3)).unwrap();
    syzygy.process_events();
    
    // Tasks should execute asynchronously and not block event loop
    assert_eq!(syzygy.state().counter, 3);
    
    // Wait for async task to complete and dispatch back
    sleep(Duration::from_millis(50)).await;
    syzygy.process_events();
    
    // Async task should have dispatched another increment
    assert_eq!(syzygy.state().counter, 6);
}

// SYZ-013: Effect Isolation
#[tokio::test]
async fn test_effect_isolation() {
    let (mut syzygy, handle) = Syzygy::builder()
        .with_state(TestState::default())
        .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
            match event {
                TestEvent::Increment(value) => {
                    // Pure event processing - no side effects here
                    state.counter += value;
                    // Side effects isolated in tasks
                    Dispatch::tasks_only(vec![TestTask::LogEvent("increment".to_string())])
                }
                TestEvent::ProcessingComplete => {
                    state.events_processed.push("complete".to_string());
                    Dispatch::empty()
                }
            }
        })
        .build();
    
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    
    // Event loop should remain unaffected by task execution
    let start = std::time::Instant::now();
    syzygy.process_events();
    let process_time = start.elapsed();
    
    // Event processing should be fast (effects isolated)
    assert!(process_time < Duration::from_millis(1));
    assert_eq!(syzygy.state().counter, 1);
    
    // Tasks communicate back via events
    sleep(Duration::from_millis(20)).await;
    syzygy.process_events();
    assert!(syzygy.state().events_processed.contains(&"complete".to_string()));
}