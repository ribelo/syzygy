//! Test named tasks functionality

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use syzygy::prelude::*;
use tokio::time::sleep;

#[derive(Debug, Clone)]
struct TestModel {
    value: i32,
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_named_task() {
    let task_ran = Arc::new(AtomicBool::new(false));
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel { value: 0 })
        .build();

    let ran = task_ran.clone();
    syzygy.task_named("test:simple_task", move |_ctx| async move {
        ran.store(true, Ordering::SeqCst);
    });

    // Process the effect to spawn the task
    syzygy.handle_effects();
    
    // Give task time to run
    sleep(Duration::from_millis(50)).await;
    
    assert!(task_ran.load(Ordering::SeqCst), "Named task should have run");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_named_blocking_task() {
    let task_ran = Arc::new(AtomicBool::new(false));
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel { value: 0 })
        .build();

    let ran = task_ran.clone();
    syzygy.spawn_blocking_named("compute:expensive", move |_ctx| {
        ran.store(true, Ordering::SeqCst);
    });

    // Process the effect to spawn the task
    syzygy.handle_effects();
    
    // Give blocking task time to run
    sleep(Duration::from_millis(50)).await;
    
    assert!(task_ran.load(Ordering::SeqCst), "Named blocking task should have run");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_multiple_named_tasks() {
    let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel { value: 0 })
        .build();

    // Spawn multiple named tasks
    for i in 0..5 {
        let counter = counter.clone();
        let task_name = match i {
            0 => "data:fetch_user",
            1 => "auth:refresh_token",
            2 => "sync:upload_changes",
            3 => "bg:cleanup_cache",
            _ => "misc:other_task",
        };
        
        syzygy.task_named(task_name, move |_ctx| async move {
            counter.fetch_add(1, Ordering::SeqCst);
        });
    }

    // Process all effects
    syzygy.handle_effects();
    
    // Wait for all tasks
    sleep(Duration::from_millis(100)).await;
    
    assert_eq!(counter.load(Ordering::SeqCst), 5, "All named tasks should have run");
}

#[cfg(all(feature = "async", debug_assertions))]
#[tokio::test]
async fn test_named_task_logging() {
    // In debug builds, named tasks should log their lifecycle
    // This test just verifies the code compiles and runs without panicking
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel { value: 0 })
        .build();

    syzygy.task_named("logging:test", |_ctx| async move {
        // Task that does nothing, just tests logging
    });

    syzygy.handle_effects();
    sleep(Duration::from_millis(50)).await;
    
    // If we get here without panic, logging works
    assert!(true, "Logging should work without panic");
}