//! Basic tests for the simplified Task API

use syzygy::executor::{InlineAsync, Task, Outcome};
use syzygy::scheduler::{BlockingScheduler, Scheduler};
use syzygy::executor::spec::{Concurrency, Fallback};

/// Test that BlockingScheduler correctly reports no overlap support
#[test]
fn test_blocking_scheduler_allows_overlap_returns_false() {
    let scheduler = BlockingScheduler;
    assert!(!scheduler.allows_overlap(),
        "BlockingScheduler must report that it doesn't allow overlap");
}

/// Test task creation methods work correctly
#[test]
fn test_task_creation_methods() {
    // Test best_effort creates task with correct defaults
    let task = Task::<(), ()>::best_effort::<InlineAsync<()>, _>(
        async move { Outcome::None }
    );

    match task {
        Task::Future { concurrency, fallback, .. } => {
            assert_eq!(concurrency, Concurrency::BestEffort,
                "best_effort() must set BestEffort concurrency");
            assert!(matches!(fallback, Fallback::Inline),
                "best_effort() must set Inline fallback");
        }
        _ => panic!("Expected Future task"),
    }

    // Test concurrent creates task with correct defaults
    let task = Task::<(), ()>::concurrent::<InlineAsync<()>, _>(
        async move { Outcome::None }
    );

    match task {
        Task::Future { concurrency, fallback, .. } => {
            assert_eq!(concurrency, Concurrency::MustOverlap,
                "concurrent() must set MustOverlap concurrency");
            assert!(matches!(fallback, Fallback::None),
                "concurrent() must set None fallback");
        }
        _ => panic!("Expected Future task"),
    }

    // Test best_effort_with creates task with correct defaults
    let task = Task::<(), ()>::best_effort_with::<InlineAsync<()>, _, _>(
        |_ctx| async move { Outcome::None }
    );

    match task {
        Task::Future { concurrency, fallback, .. } => {
            assert_eq!(concurrency, Concurrency::BestEffort,
                "best_effort_with() must set BestEffort concurrency");
            assert!(matches!(fallback, Fallback::Inline),
                "best_effort_with() must set Inline fallback");
        }
        _ => panic!("Expected Future task"),
    }

    // Test concurrent_with creates task with correct defaults
    let task = Task::<(), ()>::concurrent_with::<InlineAsync<()>, _, _>(
        |_ctx| async move { Outcome::None }
    );

    match task {
        Task::Future { concurrency, fallback, .. } => {
            assert_eq!(concurrency, Concurrency::MustOverlap,
                "concurrent_with() must set MustOverlap concurrency");
            assert!(matches!(fallback, Fallback::None),
                "concurrent_with() must set None fallback");
        }
        _ => panic!("Expected Future task"),
    }
}