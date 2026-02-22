use std::time::Duration;

use futures_util::future::BoxFuture;
use tokio::runtime::Handle;

use super::{AsyncExecutor, BlockingExecutor, ExecutorError, ExecutorLifecycle};

/// Tokio-backed executor adapter.
///
/// This wrapper borrows an existing runtime handle; lifecycle ownership stays
/// with the caller that created the runtime.
#[derive(Clone)]
pub struct TokioExecutor {
    handle: Handle,
}

impl TokioExecutor {
    #[must_use]
    pub fn new(handle: Handle) -> Self {
        Self { handle }
    }

    #[must_use]
    pub fn from_current() -> Self {
        Self::new(Handle::current())
    }
}

impl std::fmt::Debug for TokioExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokioExecutor").finish()
    }
}

impl AsyncExecutor for TokioExecutor {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        drop(self.handle.spawn(job));
        Ok(())
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(duration))
    }
}

impl BlockingExecutor for TokioExecutor {
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError> {
        drop(self.handle.spawn_blocking(job));
        Ok(())
    }
}

impl ExecutorLifecycle for TokioExecutor {
    fn shutdown(&self) {
        // Runtime lifecycle is owned by the creator of `Handle`.
    }

    fn wait(&self) {
        // We cannot drain tasks for a runtime we do not own.
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::TokioExecutor;
    use crate::executor::{AsyncExecutor, BlockingExecutor};

    #[tokio::test(flavor = "multi_thread")]
    async fn tokio_executor_runs_async_task() {
        let executor = TokioExecutor::from_current();
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

        let spawn_result = executor.spawn_async(Box::pin(async move {
            let _ = tx.send(7);
        }));

        assert!(spawn_result.is_ok());

        let received = tokio::time::timeout(Duration::from_secs(1), rx).await;
        assert!(matches!(received, Ok(Ok(7))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tokio_executor_sleep_works() {
        let executor = TokioExecutor::from_current();
        let start = Instant::now();

        executor.sleep(Duration::from_millis(10)).await;

        assert!(start.elapsed() >= Duration::from_millis(10));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tokio_executor_runs_blocking_task() {
        let executor = TokioExecutor::from_current();
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

        let spawn_result = executor.spawn_blocking(Box::new(move || {
            let _ = tx.send(9);
        }));

        assert!(spawn_result.is_ok());

        let received = tokio::time::timeout(Duration::from_secs(1), rx).await;
        assert!(matches!(received, Ok(Ok(9))));
    }
}
