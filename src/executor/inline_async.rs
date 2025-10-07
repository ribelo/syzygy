use std::marker::PhantomData;
use std::time::Duration;

use futures::executor::block_on;
use futures::future::{BoxFuture, FutureExt};

use super::{AsyncExecutor, ExecutorError, ExecutorLifecycle};

/// Inline async executor - executes jobs immediately on the caller thread.
pub struct InlineAsync<E> {
    _marker: PhantomData<E>,
}

impl<E> Default for InlineAsync<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> InlineAsync<E> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<E> std::fmt::Debug for InlineAsync<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineAsync").finish()
    }
}

impl<E> AsyncExecutor<E> for InlineAsync<E>
where
    E: Send + Sync + 'static,
{
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        block_on(job);
        Ok(())
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        async move { std::thread::sleep(duration) }.boxed()
    }
}

impl<E> ExecutorLifecycle for InlineAsync<E>
where
    E: Send + Sync + 'static,
{
    fn shutdown(&self) {}

    fn wait(&self) {
        // inline executor completes work immediately; nothing to wait on
    }
}
