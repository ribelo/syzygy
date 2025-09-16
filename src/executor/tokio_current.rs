//! TokioCurrent executor - uses current runtime only

use crate::executor::{
    AbortOnDrop, AsyncExecutor, Concurrent, ExecutorError, ExecutorLifecycle, Outcome,
};
use futures_util::future::BoxFuture;
use tokio::runtime::Handle;

/// Executor that uses the current tokio runtime only
#[derive(Clone, Debug)]
pub struct TokioCurrent {
    handle: Handle,
}

impl Concurrent for TokioCurrent {}

impl TokioCurrent {
    pub fn new() -> Result<Self, &'static str> {
        Handle::try_current()
            .map_err(
                |_| "No tokio runtime is running. Use #[tokio::main] or create a runtime first.",
            )
            .map(|handle| Self { handle })
    }
}

impl<E> AsyncExecutor<E> for TokioCurrent
where
    E: Send + 'static,
{
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, Outcome<E>>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>> {
        let (ab_fut, abort_handle) = futures_util::future::abortable(fut);
        let join_handle = self.handle.spawn(ab_fut);

        Box::pin(AbortOnDrop {
            handle: join_handle,
            abort: abort_handle,
        })
    }
}

impl ExecutorLifecycle for TokioCurrent {
    fn shutdown(&self) {
        // No-op - we don't own the runtime
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        Box::pin(futures_util::future::ready(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::AsyncExecutor;

    #[tokio::test]
    async fn test_tokio_current_uses_existing_runtime() {
        // Should succeed when runtime exists
        let executor = TokioCurrent::new().expect("Should create executor in tokio::test");

        // Test spawning a simple future
        let fut = Box::pin(async { Outcome::Events(vec![42]) });
        let result_fut = executor.spawn_future(fut);

        let result = result_fut.await;
        assert!(result.is_ok());
        match result.unwrap() {
            Outcome::Events(events) => assert_eq!(events, vec![42]),
            _ => panic!("Expected Events outcome"),
        }
    }

    #[test]
    fn test_tokio_current_fails_without_runtime() {
        // Should fail when no runtime exists
        let result = TokioCurrent::new();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "No tokio runtime is running. Use #[tokio::main] or create a runtime first."
        );
    }

    #[tokio::test]
    async fn test_tokio_current_lifecycle_is_noop() {
        let executor = TokioCurrent::new().expect("Should create executor");

        // Shutdown should be no-op (we don't own the runtime)
        executor.shutdown();

        // Join should complete immediately
        executor.join().await;

        // Should still work after shutdown (runtime is external)
        let fut = Box::pin(async { Outcome::<i32>::None });
        let result = executor.spawn_future(fut).await;
        assert!(result.is_ok());
    }
}
