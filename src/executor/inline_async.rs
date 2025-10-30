use std::time::Duration;

use futures::executor::block_on;
use futures::future::{BoxFuture, FutureExt};

use super::{AsyncExecutor, ExecutorError, ExecutorLifecycle};

/// Inline async executor — executes futures immediately on the caller thread.
///
/// Great for tests and CLIs where determinism beats concurrency. `sleep()`
/// blocks the current thread.
pub struct InlineAsync;

impl Default for InlineAsync {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineAsync {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Debug for InlineAsync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineAsync").finish()
    }
}

impl AsyncExecutor for InlineAsync {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        block_on(job);
        Ok(())
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        async move { std::thread::sleep(duration) }.boxed()
    }
}

impl ExecutorLifecycle for InlineAsync {
    fn shutdown(&self) {}

    fn wait(&self) {
        // inline executor completes work immediately; nothing to wait on
    }
}
