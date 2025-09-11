//! TokioCurrent executor - uses current runtime only

use crate::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle, Outcome};
use futures_util::future::{AbortHandle, BoxFuture};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

/// Executor that uses the current tokio runtime only
#[derive(Clone)]
pub struct TokioCurrent {
    handle: Handle,
}

impl TokioCurrent {
    pub fn new() -> Result<Self, &'static str> {
        Handle::try_current()
            .map_err(|_| "No tokio runtime is running. Use #[tokio::main] or create a runtime first.")
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

/// Helper struct to abort futures when dropped
struct AbortOnDrop<T> {
    handle: JoinHandle<Result<T, futures_util::future::Aborted>>,
    abort: AbortHandle,
}

impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, ExecutorError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll the join handle
        match Pin::new(&mut self.handle).poll(cx) {
            Poll::Ready(result) => match result {
                Ok(Ok(value)) => Poll::Ready(Ok(value)),
                Ok(Err(_)) => Poll::Ready(Err(ExecutorError::Cancelled)),
                Err(e) => Poll::Ready(Err(ExecutorError::Panic {
                    msg: format!("Join error: {e}"),
                })),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.abort.abort();
    }
}