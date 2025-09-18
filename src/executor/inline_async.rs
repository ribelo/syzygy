//! Inline async executor - runs futures without spawning
//!
//! This executor provides "asynchrony without concurrency" by running
//! futures inline rather than spawning them. Perfect for single-threaded
//! environments or when concurrency is not required.

use super::{AsyncExecutor, ExecutorError, ExecutorLifecycle, Outcome, Sequential};
use futures::executor::block_on;
use futures::future::{BoxFuture, FutureExt, ready};
use std::marker::PhantomData;
use std::time::Duration;

#[cfg(feature = "tokio")]
use tokio::{
    runtime::{Handle, RuntimeFlavor},
    task::block_in_place,
};

/// An executor that runs futures inline without spawning
///
/// This doesn't create any concurrency - futures are executed
/// immediately when spawn_future is called. Use this when you
/// want async code to run synchronously.
pub struct InlineAsync<E> {
    _phantom: PhantomData<E>,
}

impl<E: 'static> Sequential for InlineAsync<E> {}

impl<E> InlineAsync<E> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<E> Default for InlineAsync<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> std::fmt::Debug for InlineAsync<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineAsync").finish()
    }
}

impl<E> Clone for InlineAsync<E> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl<E: Send + Sync + 'static> AsyncExecutor<E> for InlineAsync<E> {
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, Outcome<E>>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>> {
        // Don't actually spawn - return a future that runs the original inline
        async move { Ok(fut.await) }.boxed()
    }

    fn spawn_detached(&self, fut: BoxFuture<'static, ()>) {
        #[cfg(feature = "tokio")]
        {
            if let Ok(handle) = Handle::try_current()
                && handle.runtime_flavor() == RuntimeFlavor::MultiThread
            {
                block_in_place(move || block_on(fut));
                return;
            }
        }

        block_on(fut);
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        async move { std::thread::sleep(duration) }.boxed()
    }

    fn allows_overlap(&self) -> bool {
        false
    }
}

impl<E: Send + Sync + 'static> ExecutorLifecycle for InlineAsync<E> {
    fn shutdown(&self) {
        // No-op - nothing to shutdown
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        ready(()).boxed()
    }
}
