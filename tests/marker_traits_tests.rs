//! Comprehensive tests for marker trait safety and functionality
//!
//! This module tests the `Concurrent` and `Sequential` marker traits that prevent
//! compile-time footguns when using executors with different concurrency guarantees.

use syzygy::executor::{
    AsyncExecutor, Concurrent, ExecutorLifecycle, ExecutorRegistry, InlineAsync, Outcome,
    Sequential, Task,
};

#[cfg(feature = "tokio")]
use syzygy::executor::TokioExecutor;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Test event types for marker trait testing
#[derive(Debug, Clone, PartialEq)]
enum TestEvent {
    Started { id: usize },
    Completed { id: usize },
    Error { message: String },
}

// ============================================================================
// Core Marker Trait Tests - This is what we're actually testing
// ============================================================================

/// Test that Sequential executors implement the Sequential marker trait
#[test]
fn test_sequential_marker_trait_implementations() {
    fn requires_sequential<E: AsyncExecutor<TestEvent> + Sequential>(_executor: E) {}

    // ✅ InlineAsync should implement Sequential
    requires_sequential(InlineAsync::<TestEvent>::new());

    // This test passes if it compiles - the type system enforces the constraint
}

/// Test that Concurrent executors implement the Concurrent marker trait
#[cfg(feature = "tokio")]
#[test]
fn test_concurrent_marker_trait_implementations() {
    fn requires_concurrent<E: AsyncExecutor<TestEvent> + Concurrent>(_executor: E) {}

    // ✅ All Tokio executors should implement Concurrent
    requires_concurrent(TokioExecutor::current_thread_io("test"));
    requires_concurrent(TokioExecutor::current_thread_io("io-test"));
    requires_concurrent(TokioExecutor::current_thread_cpu("cpu-test"));

    // This test passes if it compiles - the type system enforces the constraint
}

/// Test Task::async_task works with any async executor
#[test]
fn test_task_async_task_works_with_any_executor() {
    // ✅ Any async executor should work
    let _task1 = Task::<TestEvent, ()>::async_task::<InlineAsync<TestEvent>, _>(async {
        Outcome::Event(TestEvent::Started { id: 1 })
    });

    #[cfg(feature = "tokio")]
    {
        // ✅ Tokio executors should also work
        let _task2 = Task::<TestEvent, ()>::async_task::<TokioExecutor, _>(async {
            Outcome::Event(TestEvent::Started { id: 2 })
        });
    }

    // If this compiles, the simplified API works correctly
}

/// Test that the simplified API works with various executors
#[cfg(feature = "tokio")]
#[test]
fn test_simplified_api_works_with_tokio_executors() {
    // ✅ All executors work with the simplified API
    let _task1 = Task::<TestEvent, ()>::async_task::<TokioExecutor, _>(async {
        Outcome::Event(TestEvent::Started { id: 1 })
    });

    let _task2 = Task::<TestEvent, ()>::async_task::<TokioExecutor, _>(async {
        Outcome::Event(TestEvent::Started { id: 2 })
    });

    // If this compiles, the simplified API works correctly
}

// ============================================================================
// Runtime Behavior Tests
// ============================================================================

/// Test that Sequential executors actually execute sequentially
#[tokio::test]
async fn test_sequential_executor_runtime_behavior() {
    let execution_order = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let order_clone = Arc::clone(&execution_order);

    let inline_executor = InlineAsync::<TestEvent>::new();

    // Create futures that would overlap if run concurrently
    let fut1 = {
        let order = Arc::clone(&order_clone);
        async move {
            order.lock().unwrap().push("task1_start".to_string());
            // Small async operation
            tokio::task::yield_now().await;
            order.lock().unwrap().push("task1_end".to_string());
            Outcome::Event(TestEvent::Completed { id: 1 })
        }
    };

    let fut2 = {
        let order = Arc::clone(&order_clone);
        async move {
            order.lock().unwrap().push("task2_start".to_string());
            tokio::task::yield_now().await;
            order.lock().unwrap().push("task2_end".to_string());
            Outcome::Event(TestEvent::Completed { id: 2 })
        }
    };

    // Execute using the sequential executor - they should run one after another
    let result1 = inline_executor.spawn_future(Box::pin(fut1)).await;
    let result2 = inline_executor.spawn_future(Box::pin(fut2)).await;

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let order = execution_order.lock().unwrap();
    // With InlineAsync, tasks should execute completely sequentially
    assert_eq!(
        *order,
        vec![
            "task1_start".to_string(),
            "task1_end".to_string(),
            "task2_start".to_string(),
            "task2_end".to_string()
        ]
    );
}

/// Test that Concurrent executors can handle overlapping execution
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_concurrent_executor_runtime_behavior() {
    let executor = TokioExecutor::multi_thread_io("test", 2);
    let completed_count = Arc::new(AtomicUsize::new(0));

    // Create futures with different delays for concurrent execution
    let futures: Vec<_> = (0..4)
        .map(|i| {
            let count = Arc::clone(&completed_count);
            Box::pin(async move {
                // Different delays to test concurrency
                tokio::time::sleep(Duration::from_millis(10 + (i as u64) * 5)).await;
                count.fetch_add(1, Ordering::SeqCst);
                Outcome::Event(TestEvent::Completed { id: i })
            })
        })
        .collect();

    let start_time = Instant::now();

    // Execute all futures through the concurrent executor
    let results =
        futures_util::future::join_all(futures.into_iter().map(|fut| executor.spawn_future(fut)))
            .await;

    let elapsed = start_time.elapsed();

    // All tasks should complete successfully
    for result in results {
        assert!(result.is_ok());
    }

    assert_eq!(completed_count.load(Ordering::SeqCst), 4);

    // With concurrent execution, this should be much faster than sequential
    // Sequential would take: 10+15+20+25 = 70ms
    // Concurrent should take ~25ms (longest task)
    assert!(
        elapsed < Duration::from_millis(50),
        "Concurrent execution should be faster. Took: {elapsed:?}"
    );

    executor.shutdown();
    executor.join().await;
}

// ============================================================================
// Executor Registry Tests
// ============================================================================

/// Test that ExecutorRegistry works correctly with different marker trait types
#[test]
fn test_executor_registry_with_marker_traits() {
    let mut registry = ExecutorRegistry::<TestEvent>::new();

    // Register sequential executor
    registry.insert_async(InlineAsync::<TestEvent>::new());

    #[cfg(feature = "tokio")]
    {
        // Register concurrent executor
        registry.insert_async(TokioExecutor::current_thread_io("test"));
    }

    // Verify executors can be retrieved by type
    assert!(registry.async_exec::<InlineAsync<TestEvent>>().is_some());

    #[cfg(feature = "tokio")]
    {
        assert!(registry.async_exec::<TokioExecutor>().is_some());
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test that marker traits don't interfere with error handling
#[tokio::test]
async fn test_marker_traits_with_errors() {
    let inline_executor = InlineAsync::<TestEvent>::new();

    // Test error handling in sequential execution
    let error_future = async {
        Outcome::Event(TestEvent::Error {
            message: "Test error".to_string(),
        })
    };

    let result = inline_executor.spawn_future(Box::pin(error_future)).await;
    assert!(result.is_ok());

    match result.unwrap() {
        Outcome::Event(TestEvent::Error { message }) => {
            assert_eq!(message, "Test error");
        }
        _ => panic!("Expected error event"),
    }
}

// ============================================================================
// Stress Tests
// ============================================================================

/// Stress test to verify marker traits work under load
#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_marker_traits_stress_test() {
    let executor = TokioExecutor::multi_thread_io("stress", 4);
    let completed_count = Arc::new(AtomicUsize::new(0));

    // Create many concurrent tasks
    let futures: Vec<_> = (0..100)
        .map(|i| {
            let count = Arc::clone(&completed_count);
            Box::pin(async move {
                // Minimal delay for stress testing
                tokio::time::sleep(Duration::from_millis(1)).await;
                count.fetch_add(1, Ordering::SeqCst);
                Outcome::Event(TestEvent::Completed { id: i })
            })
        })
        .collect();

    let start_time = Instant::now();

    // Execute all futures concurrently
    let results =
        futures_util::future::join_all(futures.into_iter().map(|fut| executor.spawn_future(fut)))
            .await;

    let elapsed = start_time.elapsed();

    // All should complete successfully
    assert_eq!(results.len(), 100);
    for result in results {
        assert!(result.is_ok());
    }

    assert_eq!(completed_count.load(Ordering::SeqCst), 100);

    // Should complete reasonably fast
    assert!(
        elapsed < Duration::from_millis(500),
        "Stress test took too long: {elapsed:?}"
    );

    executor.shutdown();
    executor.join().await;
}

// ============================================================================
// Compile-time Safety Documentation Tests
// ============================================================================

/// Test that demonstrates all the compile-time safety guarantees
///
/// This serves as both a test and documentation of what should/shouldn't compile
#[test]
fn test_compile_time_safety_guarantees() {
    // ✅ Any executor works with simplified API
    let _sequential = InlineAsync::<TestEvent>::new();
    let _task1 = Task::<TestEvent, ()>::async_task::<InlineAsync<TestEvent>, _>(async {
        Outcome::Event(TestEvent::Started { id: 1 })
    });

    #[cfg(feature = "tokio")]
    {
        // ✅ All Tokio executors work with simplified API
        let _task2 = Task::<TestEvent, ()>::async_task::<TokioExecutor, _>(async {
            Outcome::Event(TestEvent::Started { id: 2 })
        });

        let _task3 = Task::<TestEvent, ()>::async_task::<TokioExecutor, _>(async {
            Outcome::Event(TestEvent::Started { id: 3 })
        });

        let _task4 = Task::<TestEvent, ()>::async_task::<TokioExecutor, _>(async {
            Outcome::Event(TestEvent::Started { id: 4 })
        });
    }

    // Simplified API - no more compile-time restrictions
}

/// Helper function to verify trait bounds at compile time
fn _compile_time_trait_verification() {
    // These functions exist only for compile-time verification
    // They should never be called, but must compile to pass the test

    fn sequential_only<E: AsyncExecutor<TestEvent> + Sequential>(_: E) {}
    fn concurrent_only<E: AsyncExecutor<TestEvent> + Concurrent>(_: E) {}

    // ✅ These should compile
    sequential_only(InlineAsync::<TestEvent>::new());

    #[cfg(feature = "tokio")]
    {
        concurrent_only(TokioExecutor::current_thread_io("test"));
        concurrent_only(TokioExecutor::current_thread_io("io-test"));
        concurrent_only(TokioExecutor::current_thread_cpu("cpu-test"));
    }

    // ❌ These would NOT compile:
    // _concurrent_only(InlineAsync::<TestEvent>::new());
    // Error: "the trait bound `InlineAsync<TestEvent>: Concurrent` is not satisfied"
}
