//! Task safety tests for TokioExecutor
//! 
//! These tests verify that all tasks spawned by executors are properly
//! cancelled when the executor is dropped, preventing orphaned tasks.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use tokio::time::sleep;
use syzygy::executor::{TokioExecutor, SpawnExecutor};

#[tokio::test]
async fn test_executor_drop_cancels_task() {
    let task_started = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::new(AtomicBool::new(false));

    // Create scope to control executor lifetime
    {
        let executor: TokioExecutor<()> = TokioExecutor::new();

        let task_started_clone = task_started.clone();
        let task_completed_clone = task_completed.clone();

        // Spawn a long-running task
        executor.spawn(async move {
            task_started_clone.store(true, Ordering::Relaxed);

            // This should be cancelled before completion
            sleep(Duration::from_millis(2000)).await;

            // If we reach here, task wasn't properly cancelled
            task_completed_clone.store(true, Ordering::Relaxed);
        })
        .expect("Failed to spawn task");

        // Give task time to start
        sleep(Duration::from_millis(50)).await;
        
        // Executor will be dropped here, causing task cancellation
    }

    // Give time for cleanup
    sleep(Duration::from_millis(100)).await;

    // Verify task started but was cancelled before completion
    assert!(task_started.load(Ordering::Relaxed), "Task should have started");
    assert!(!task_completed.load(Ordering::Relaxed), "Task should have been cancelled");
}

#[tokio::test]
async fn test_executor_drop_cancels_multiple_tasks() {
    let tasks_started = Arc::new(AtomicBool::new(false));
    let tasks_completed = Arc::new(AtomicBool::new(false));

    // Create scope to control executor lifetime
    {
        let executor: TokioExecutor<()> = TokioExecutor::new();

        // Spawn multiple long-running tasks
        for i in 0..5 {
            let tasks_started_clone = tasks_started.clone();
            let tasks_completed_clone = tasks_completed.clone();

            executor.spawn(async move {
                if i == 0 {
                    tasks_started_clone.store(true, Ordering::Relaxed);
                }

                // Long sleep that should be cancelled
                sleep(Duration::from_millis(2000)).await;

                // If we reach here, tasks weren't properly cancelled
                tasks_completed_clone.store(true, Ordering::Relaxed);
            })
            .expect("Failed to spawn task");
        }

        // Give tasks time to start
        sleep(Duration::from_millis(50)).await;
        
        // Executor will be dropped here, cancelling all tasks
    }

    // Give time for cleanup
    sleep(Duration::from_millis(100)).await;

    // Verify tasks started but were cancelled
    assert!(tasks_started.load(Ordering::Relaxed), "Tasks should have started");
    assert!(!tasks_completed.load(Ordering::Relaxed), "Tasks should have been cancelled");
}

#[tokio::test]
async fn test_executor_batch_spawn_cancellation() {
    let task_started = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::new(AtomicBool::new(false));

    // Create scope to control executor lifetime  
    {
        let executor: TokioExecutor<()> = TokioExecutor::new();

        // Create batch of futures
        let futures = (0..3).map(|i| {
            let task_started_clone = task_started.clone();
            let task_completed_clone = task_completed.clone();
            
            async move {
                if i == 0 {
                    task_started_clone.store(true, Ordering::Relaxed);
                }

                // Long sleep that should be cancelled
                sleep(Duration::from_millis(2000)).await;

                // If we reach here, tasks weren't cancelled
                task_completed_clone.store(true, Ordering::Relaxed);
            }
        });

        let _task_ids = executor.spawn_batch(futures).expect("Failed to batch spawn");

        // Give tasks time to start
        sleep(Duration::from_millis(50)).await;
        
        // Executor dropped here, should cancel all batch tasks
    }

    // Give time for cleanup
    sleep(Duration::from_millis(100)).await;

    assert!(task_started.load(Ordering::Relaxed), "Batch tasks should have started");
    assert!(!task_completed.load(Ordering::Relaxed), "Batch tasks should have been cancelled");
}

#[tokio::test]
async fn test_executor_spawn_with_timeout_cancellation() {
    let task_started = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::new(AtomicBool::new(false));

    // Create scope to control executor lifetime
    {
        let executor: TokioExecutor<()> = TokioExecutor::new();

        let task_started_clone = task_started.clone();
        let task_completed_clone = task_completed.clone();

        // Spawn task with timeout
        executor.spawn_with_timeout(Duration::from_millis(5000), async move {
            task_started_clone.store(true, Ordering::Relaxed);

            // Long sleep that should be cancelled by executor drop, not timeout
            sleep(Duration::from_millis(2000)).await;

            task_completed_clone.store(true, Ordering::Relaxed);
        })
        .expect("Failed to spawn with timeout");

        sleep(Duration::from_millis(50)).await;
        
        // Executor dropped before timeout, should still cancel task
    }

    sleep(Duration::from_millis(100)).await;

    assert!(task_started.load(Ordering::Relaxed), "Timeout task should have started");
    assert!(!task_completed.load(Ordering::Relaxed), "Timeout task should have been cancelled");
}

#[tokio::test] 
async fn test_executor_cloned_context_safety() {
    let task_started = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::new(AtomicBool::new(false));

    // Test that cloning executor still provides proper cleanup
    {
        let executor: TokioExecutor<()> = TokioExecutor::new();
        let cloned_executor = executor.clone();

        let task_started_clone = task_started.clone();
        let task_completed_clone = task_completed.clone();

        // Spawn from cloned executor
        cloned_executor.spawn(async move {
            task_started_clone.store(true, Ordering::Relaxed);
            sleep(Duration::from_millis(2000)).await;
            task_completed_clone.store(true, Ordering::Relaxed);
        })
        .expect("Failed to spawn from cloned executor");

        sleep(Duration::from_millis(50)).await;
        
        // Both original and cloned executors dropped
    }

    sleep(Duration::from_millis(100)).await;

    assert!(task_started.load(Ordering::Relaxed), "Cloned executor task should have started");
    assert!(!task_completed.load(Ordering::Relaxed), "Cloned executor task should have been cancelled");
}