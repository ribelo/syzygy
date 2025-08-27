//! Task Safety Tests
//!
//! These tests verify that AsyncContextV2 properly cancels spawned tasks
//! when the context is dropped, preventing orphaned tasks and memory safety issues.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use tokio::time::sleep;

use syzygy::async_context::EffectContext;
use syzygy::storage::EmptyStorage;

/// Test that tasks are cancelled when EffectContext is dropped
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_tasks_cancelled_on_context_drop() {
    let task_started = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::new(AtomicBool::new(false));
    let task_cancelled = Arc::new(AtomicBool::new(false));

    // Create scope to control context lifetime
    {
        let ctx: EffectContext<(), EmptyStorage> =
            EffectContext::new(None, std::sync::Arc::new(EmptyStorage));

        let task_started_clone = task_started.clone();
        let task_completed_clone = task_completed.clone();
        let _task_cancelled_clone = task_cancelled.clone();

        // Spawn a long-running task
        ctx.spawn(async move {
            task_started_clone.store(true, Ordering::Relaxed);

            // This should be cancelled before completion
            let result = tokio::time::timeout(
                Duration::from_millis(1000),        // Long timeout
                sleep(Duration::from_millis(2000)), // Even longer sleep
            )
            .await;

            if let Ok(()) = result {
                // Task completed normally (shouldn't happen)
                task_completed_clone.store(true, Ordering::Relaxed);
            } else {
                // This shouldn't happen in our test since we're using abort
                // Tokio's abort doesn't use timeout, it kills immediately
            }

            // If we reach here, task wasn't properly cancelled
            task_completed_clone.store(true, Ordering::Relaxed);
        })
        .expect("Failed to spawn task");

        // Let the task start
        sleep(Duration::from_millis(50)).await;
        assert!(
            task_started.load(Ordering::Relaxed),
            "Task should have started"
        );
        assert!(
            !task_completed.load(Ordering::Relaxed),
            "Task should not have completed yet"
        );

        // Context drops here, should cancel the task
    }

    // Give some time for cleanup to occur
    sleep(Duration::from_millis(100)).await;

    // Verify task was cancelled, not completed
    assert!(
        task_started.load(Ordering::Relaxed),
        "Task should have started"
    );
    assert!(
        !task_completed.load(Ordering::Relaxed),
        "Task should have been cancelled, not completed"
    );
}

/// Test that multiple tasks are all cancelled when context drops
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_multiple_tasks_cancelled_on_drop() {
    const TASK_COUNT: u32 = 5;

    let tasks_started = Arc::new(AtomicU32::new(0));
    let tasks_completed = Arc::new(AtomicU32::new(0));

    // Create scope for context lifetime
    {
        let ctx: EffectContext<(), EmptyStorage> =
            EffectContext::new(None, std::sync::Arc::new(EmptyStorage));

        // Spawn multiple long-running tasks
        for i in 0..TASK_COUNT {
            let tasks_started_clone = tasks_started.clone();
            let tasks_completed_clone = tasks_completed.clone();

            ctx.spawn(async move {
                tasks_started_clone.fetch_add(1, Ordering::Relaxed);

                // Each task sleeps for a different duration
                sleep(Duration::from_millis(500 + (u64::from(i) * 100))).await;

                // If we reach here, task wasn't cancelled
                tasks_completed_clone.fetch_add(1, Ordering::Relaxed);
            })
            .expect("Failed to spawn task");
        }

        // Let all tasks start
        sleep(Duration::from_millis(100)).await;
        assert_eq!(
            tasks_started.load(Ordering::Relaxed),
            TASK_COUNT,
            "All tasks should have started"
        );
        assert_eq!(
            tasks_completed.load(Ordering::Relaxed),
            0,
            "No tasks should have completed yet"
        );

        // Context drops here, should cancel all tasks
    }

    // Give time for cleanup
    sleep(Duration::from_millis(200)).await;

    // Verify all tasks were cancelled
    assert_eq!(
        tasks_started.load(Ordering::Relaxed),
        TASK_COUNT,
        "All tasks should have started"
    );
    assert_eq!(
        tasks_completed.load(Ordering::Relaxed),
        0,
        "All tasks should have been cancelled"
    );
}

/// Test that tasks accessing shared data don't cause memory safety issues after context drop
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_no_use_after_free_on_context_drop() {
    let shared_data = Arc::new(AtomicU32::new(42));
    let access_count = Arc::new(AtomicU32::new(0));
    let task_started = Arc::new(AtomicBool::new(false));

    // Create scope for context lifetime
    {
        let ctx: EffectContext<(), EmptyStorage> =
            EffectContext::new(None, std::sync::Arc::new(EmptyStorage));

        let shared_data_clone = shared_data.clone();
        let access_count_clone = access_count.clone();
        let task_started_clone = task_started.clone();

        ctx.spawn(async move {
            task_started_clone.store(true, Ordering::Relaxed);

            // Try to access shared data in a loop
            for _ in 0..1000 {
                // This should be safe - Arc keeps data alive
                let _value = shared_data_clone.load(Ordering::Relaxed);
                access_count_clone.fetch_add(1, Ordering::Relaxed);

                // Small delay to allow cancellation
                sleep(Duration::from_millis(1)).await;
            }
        })
        .expect("Failed to spawn task");

        // Let task start
        sleep(Duration::from_millis(10)).await;
        assert!(
            task_started.load(Ordering::Relaxed),
            "Task should have started"
        );

        // Context drops here - task should be cancelled mid-execution
    }

    // Give time for cancellation
    sleep(Duration::from_millis(50)).await;

    let final_access_count = access_count.load(Ordering::Relaxed);

    // Verify task was cancelled before completing all 1000 accesses
    assert!(
        task_started.load(Ordering::Relaxed),
        "Task should have started"
    );
    assert!(
        final_access_count < 1000,
        "Task should have been cancelled before completing all accesses (got {final_access_count})"
    );
    assert!(
        final_access_count > 0,
        "Task should have made some accesses before cancellation"
    );

    // Verify shared data is still accessible (no use-after-free)
    assert_eq!(
        shared_data.load(Ordering::Relaxed),
        42,
        "Shared data should still be accessible"
    );
}

/// Test that batch spawned tasks are also properly cancelled
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_batch_spawned_tasks_cancelled_on_drop() {
    let tasks_started = Arc::new(AtomicU32::new(0));
    let tasks_completed = Arc::new(AtomicU32::new(0));

    {
        let ctx: EffectContext<(), EmptyStorage> =
            EffectContext::new(None, std::sync::Arc::new(EmptyStorage));

        let tasks_started_clone = tasks_started.clone();
        let tasks_completed_clone = tasks_completed.clone();

        // Create futures for batch spawn
        let futures: Vec<_> = (0..3)
            .map(|i| {
                let started = tasks_started_clone.clone();
                let completed = tasks_completed_clone.clone();

                async move {
                    started.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(500 + (i as u64 * 100))).await;
                    completed.fetch_add(1, Ordering::Relaxed);
                }
            })
            .collect();

        // Batch spawn all tasks
        let _task_ids = ctx.spawn_batch(futures).expect("Failed to batch spawn");

        // Let tasks start
        sleep(Duration::from_millis(100)).await;
        assert_eq!(
            tasks_started.load(Ordering::Relaxed),
            3,
            "All batch tasks should have started"
        );
        assert_eq!(
            tasks_completed.load(Ordering::Relaxed),
            0,
            "No batch tasks should have completed yet"
        );

        // Context drops here
    }

    // Verify batch tasks were cancelled
    sleep(Duration::from_millis(200)).await;
    assert_eq!(
        tasks_started.load(Ordering::Relaxed),
        3,
        "All batch tasks should have started"
    );
    assert_eq!(
        tasks_completed.load(Ordering::Relaxed),
        0,
        "All batch tasks should have been cancelled"
    );
}

/// Test context cloning - tasks from both contexts should be cancelled when original drops
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_cloned_context_task_safety() {
    let task1_started = Arc::new(AtomicBool::new(false));
    let task2_started = Arc::new(AtomicBool::new(false));
    let task1_completed = Arc::new(AtomicBool::new(false));
    let task2_completed = Arc::new(AtomicBool::new(false));

    let cloned_ctx = {
        let ctx: EffectContext<(), EmptyStorage> =
            EffectContext::new(None, std::sync::Arc::new(EmptyStorage));
        let cloned_ctx = ctx.clone();

        // Spawn task from original context
        let task1_started_clone = task1_started.clone();
        let task1_completed_clone = task1_completed.clone();
        ctx.spawn(async move {
            task1_started_clone.store(true, Ordering::Relaxed);
            sleep(Duration::from_millis(500)).await;
            task1_completed_clone.store(true, Ordering::Relaxed);
        })
        .expect("Failed to spawn task from original context");

        // Spawn task from cloned context
        let task2_started_clone = task2_started.clone();
        let task2_completed_clone = task2_completed.clone();
        cloned_ctx
            .spawn(async move {
                task2_started_clone.store(true, Ordering::Relaxed);
                sleep(Duration::from_millis(500)).await;
                task2_completed_clone.store(true, Ordering::Relaxed);
            })
            .expect("Failed to spawn task from cloned context");

        sleep(Duration::from_millis(50)).await;
        assert!(
            task1_started.load(Ordering::Relaxed),
            "Task 1 should have started"
        );
        assert!(
            task2_started.load(Ordering::Relaxed),
            "Task 2 should have started"
        );

        cloned_ctx // Return cloned context, original drops here
    };

    // Original context dropped, but cloned context still alive
    // Tasks should still be running since TaskGroup is Arc-shared
    sleep(Duration::from_millis(100)).await;

    // Drop cloned context too
    drop(cloned_ctx);

    // Now all tasks should be cancelled
    sleep(Duration::from_millis(100)).await;

    assert!(
        task1_started.load(Ordering::Relaxed),
        "Task 1 should have started"
    );
    assert!(
        task2_started.load(Ordering::Relaxed),
        "Task 2 should have started"
    );
    assert!(
        !task1_completed.load(Ordering::Relaxed),
        "Task 1 should have been cancelled"
    );
    assert!(
        !task2_completed.load(Ordering::Relaxed),
        "Task 2 should have been cancelled"
    );
}

// Compile-time test for non-tokio features (best-effort cancellation)
#[cfg(not(feature = "tokio"))]
#[test]
fn test_non_tokio_safety_compilation() {
    // This test just verifies the code compiles for non-tokio runtimes
    // Actual safety testing would require runtime-specific test harnesses
    let ctx: EffectContext<()> = EffectContext::new(None);

    // Should compile but won't have hard cancellation guarantees
    let _result = std::panic::catch_unwind(|| {
        let _task_id = ctx.spawn(async {
            // This would use cooperative cancellation
        });
    });

    // For non-tokio runtimes, we document the limitation
    println!("Non-tokio runtime: Using cooperative cancellation (best effort)");
}
