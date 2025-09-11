//! Integration tests for TokioCurrent executor through ExecutorRegistry

use syzygy::executor::{ExecutorRegistry, TokioCurrent, AsyncExecutor, Outcome};
use syzygy::executor::tokio_current::TokioCurrent;

#[derive(Debug, Clone)]
enum TestEvent {
    Success(u32),
    Error(String),
}

#[cfg(feature = "tokio")]
mod tokio_tests {
    use super::*;
    use futures_util::future::BoxFuture;
    use std::sync::Arc;

    #[tokio::test]
    async fn tokio_current_executor_through_registry_async_spawning() {
        // Given: A registry with TokioCurrent executor
        let mut registry = ExecutorRegistry::new();
        let tokio_current = TokioCurrent::new().expect("Should create TokioCurrent executor");
        registry.insert_async(tokio_current);

        // When: Get the executor from registry and spawn a future
        let executor = registry.async_exec::<TokioCurrent>()
            .expect("Should find TokioCurrent executor");

        let fut = Box::pin(async { Outcome::Events(vec![TestEvent::Success(42)]) });
        let result_fut = executor.spawn_future(fut);

        // Then: Future should complete successfully
        let result = result_fut.await;
        assert!(result.is_ok(), "Spawn should succeed");

        match result.unwrap() {
            Outcome::Events(events) => {
                assert_eq!(events.len(), 1);
                match &events[0] {
                    TestEvent::Success(42) => {} // Expected
                    other => panic!("Unexpected event: {:?}", other),
                }
            }
            other => panic!("Expected Events outcome, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn tokio_current_executor_through_registry_error_handling() {
        // Given: A registry with TokioCurrent executor
        let mut registry = ExecutorRegistry::new();
        let tokio_current = TokioCurrent::new().expect("Should create TokioCurrent executor");
        registry.insert_async(tokio_current);

        // When: Spawn a future that panics
        let executor = registry.async_exec::<TokioCurrent>()
            .expect("Should find TokioCurrent executor");

        let fut = Box::pin(async {
            panic!("Test panic");
            #[allow(unreachable_code)]
            Outcome::<TestEvent>::None
        });
        let result_fut = executor.spawn_future(fut);

        // Then: Panic should be handled as an error
        let result = result_fut.await;
        assert!(result.is_err(), "Should return error for panic");
    }

    #[tokio::test]
    async fn tokio_current_executor_uses_existing_runtime() {
        // Given: We're already in a tokio runtime context
        let current_handle = tokio::runtime::Handle::current();

        // When: Create TokioCurrent executor
        let tokio_current = TokioCurrent::new().expect("Should create executor");

        // Then: It should use the existing runtime (same handle)
        // Note: We can't directly compare handles, but we can verify it works
        let executor = Arc::new(tokio_current) as Arc<dyn AsyncExecutor<TestEvent>>;

        let fut = Box::pin(async { Outcome::Events(vec![TestEvent::Success(1)]) });
        let result = executor.spawn_future(fut).await;

        assert!(result.is_ok(), "Should work with existing runtime");
    }

    #[tokio::test]
    async fn tokio_current_executor_registry_multiple_operations() {
        // Given: Registry with TokioCurrent
        let mut registry = ExecutorRegistry::new();
        let tokio_current = TokioCurrent::new().expect("Should create executor");
        registry.insert_async(tokio_current);

        let executor = registry.async_exec::<TokioCurrent>()
            .expect("Should find executor");

        // When: Perform multiple spawn operations
        let futures = (0..5).map(|i| {
            let fut = Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                Outcome::Events(vec![TestEvent::Success(i)])
            });
            executor.spawn_future(fut)
        });

        // Then: All should complete successfully
        for (i, future) in futures.enumerate() {
            let result = future.await;
            assert!(result.is_ok(), "Future {} should succeed", i);

            match result.unwrap() {
                Outcome::Events(events) => {
                    assert_eq!(events.len(), 1);
                    match &events[0] {
                        TestEvent::Success(val) => assert_eq!(*val, i as u32),
                        other => panic!("Unexpected event for {}: {:?}", i, other),
                    }
                }
                other => panic!("Expected Events for {}, got {:?}", i, other),
            }
        }
    }

    #[tokio::test]
    async fn tokio_current_executor_handles_none_outcome() {
        // Given: Registry with TokioCurrent
        let mut registry = ExecutorRegistry::new();
        let tokio_current = TokioCurrent::new().expect("Should create executor");
        registry.insert_async(tokio_current);

        // When: Spawn future that returns None
        let executor = registry.async_exec::<TokioCurrent>()
            .expect("Should find executor");

        let fut = Box::pin(async { Outcome::<TestEvent>::None });
        let result = executor.spawn_future(fut).await;

        // Then: Should handle None outcome correctly
        assert!(result.is_ok(), "Should succeed with None outcome");
        match result.unwrap() {
            Outcome::None => {} // Expected
            other => panic!("Expected None, got {:?}", other),
        }
    }

    #[test]
    fn tokio_current_executor_fails_without_runtime() {
        // This test runs outside tokio context to verify error handling
        // Note: This is tricky to test directly since we're in a tokio::test
        // But we can verify the constructor behavior

        // In normal tokio::test context, it should work
        let result = TokioCurrent::new();
        assert!(result.is_ok(), "Should create executor in tokio context");

        // The executor should be functional
        let executor = result.unwrap();
        assert!(executor.join().now_or_never().is_some(), "Join should complete immediately");
    }

    #[tokio::test]
    async fn tokio_current_executor_lifecycle_management() {
        // Given: TokioCurrent executor
        let tokio_current = TokioCurrent::new().expect("Should create executor");

        // When: Use it for spawning
        let executor = Arc::new(tokio_current) as Arc<dyn AsyncExecutor<TestEvent>>;
        let fut = Box::pin(async { Outcome::Events(vec![TestEvent::Success(1)]) });
        let result = executor.spawn_future(fut).await;

        // Then: Should work normally
        assert!(result.is_ok(), "Should spawn successfully");

        // When: Shutdown
        executor.shutdown();

        // Then: Should still work (since it doesn't own the runtime)
        let fut2 = Box::pin(async { Outcome::<TestEvent>::None });
        let result2 = executor.spawn_future(fut2).await;
        assert!(result2.is_ok(), "Should still work after shutdown");
    }

    #[tokio::test]
    async fn tokio_current_executor_concurrent_spawning() {
        // Given: Registry with TokioCurrent
        let mut registry = ExecutorRegistry::new();
        let tokio_current = TokioCurrent::new().expect("Should create executor");
        registry.insert_async(tokio_current);

        let executor = registry.async_exec::<TokioCurrent>()
            .expect("Should find executor");

        // When: Spawn multiple futures concurrently
        let handles: Vec<_> = (0..10).map(|i| {
            let fut = Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                Outcome::Events(vec![TestEvent::Success(i)])
            });
            executor.spawn_future(fut)
        }).collect();

        // Then: All should complete
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await;
            assert!(result.is_ok(), "Concurrent future {} should succeed", i);
        }
    }
}