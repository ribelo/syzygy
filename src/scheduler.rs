//! Runtime-neutral scheduling facade built on [`Runtime`](crate::runtime::Runtime).
//!
//! This module retains the public `Scheduler` trait for compatibility while the
//! concrete implementations now delegate to the unified runtime facade. Auto
//! detection no longer panics when no async runtime is active; instead, Syzygy
//! falls back to a synchronous inline executor suitable for tests and command-line
//! tools.

use std::future::Future;

use futures_util::future::BoxFuture;

use crate::runtime::{Runtime, RuntimeError};

/// Trait representing the ability to schedule futures onto an async runtime.
pub trait Scheduler: Clone + Send + Sync + 'static {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static);

    fn allows_overlap(&self) -> bool {
        true
    }
}

impl Scheduler for Runtime {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.schedule(future);
    }

    fn allows_overlap(&self) -> bool {
        self.allows_overlap()
    }
}

/// Legacy schedule function signature for backward compatibility.
pub type BoxedScheduleFn = dyn Fn(BoxFuture<'static, ()>) + Send + Sync;

/// Schedule a future using the auto-detected runtime (tokio > smol > async-std > inline).
pub fn auto_schedule<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    scheduler().schedule(future);
}

/// Return a scheduler bound to the auto-detected runtime.
#[must_use]
pub fn scheduler() -> Runtime {
    Runtime::current()
}

/// Strict scheduler creation: errors if no runtime is available.
pub fn scheduler_strict() -> Result<Runtime, RuntimeError> {
    Runtime::strict()
}

/// Legacy helper returning a boxed schedule function.
pub fn auto_schedule_fn() -> impl Fn(BoxFuture<'static, ()>) + Clone + Send + Sync + 'static {
    let runtime = scheduler();
    move |future| runtime.schedule(future)
}

/// Legacy helper returning a boxed schedule function identical to [`auto_schedule_fn`].
pub fn auto_schedule_boxed() -> impl Fn(BoxFuture<'static, ()>) + Clone + Send + Sync + 'static {
    auto_schedule_fn()
}

/// Scheduler backed by tokio's current runtime handle.
#[cfg(feature = "tokio")]
#[derive(Clone)]
pub struct TokioScheduler {
    runtime: Runtime,
}

#[cfg(feature = "tokio")]
impl TokioScheduler {
    pub fn new() -> Result<Self, RuntimeError> {
        Runtime::tokio().map(|runtime| Self { runtime })
    }
}

#[cfg(feature = "tokio")]
impl Scheduler for TokioScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.runtime.schedule(future);
    }
}

/// Scheduler backed by smol's global executor.
#[cfg(feature = "smol")]
#[derive(Clone, Debug)]
pub struct SmolScheduler {
    runtime: Runtime,
}

#[cfg(feature = "smol")]
impl Default for SmolScheduler {
    fn default() -> Self {
        Self {
            runtime: Runtime::smol(),
        }
    }
}

#[cfg(feature = "smol")]
impl Scheduler for SmolScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.runtime.schedule(future);
    }
}

/// Scheduler backed by async-std's global executor.
#[cfg(feature = "async-std")]
#[derive(Clone, Debug)]
pub struct AsyncStdScheduler {
    runtime: Runtime,
}

#[cfg(feature = "async-std")]
impl Default for AsyncStdScheduler {
    fn default() -> Self {
        Self {
            runtime: Runtime::async_std(),
        }
    }
}

#[cfg(feature = "async-std")]
impl Scheduler for AsyncStdScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.runtime.schedule(future);
    }
}

/// Inline scheduler that runs futures synchronously on the current thread.
#[derive(Debug, Clone, Copy)]
pub struct BlockingScheduler;

impl Scheduler for BlockingScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        futures::executor::block_on(future);
    }

    fn allows_overlap(&self) -> bool {
        false
    }
}

#[must_use]
pub fn blocking_scheduler() -> BlockingScheduler {
    BlockingScheduler
}

/// Convenience helper mirroring the legacy tokio-specific function.
#[cfg(feature = "tokio")]
pub fn schedule_tokio<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    TokioScheduler::new()
        .expect("No tokio runtime is running. Use #[tokio::main] or create a runtime first.")
        .schedule(future);
}

#[cfg(feature = "smol")]
pub fn schedule_smol<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    SmolScheduler::default().schedule(future);
}

#[cfg(feature = "async-std")]
pub fn schedule_async_std<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    AsyncStdScheduler::default().schedule(future);
}

#[cfg(feature = "tokio")]
pub fn schedule_tokio_boxed(future: BoxFuture<'static, ()>) {
    schedule_tokio(future);
}

#[cfg(feature = "smol")]
pub fn schedule_smol_boxed(future: BoxFuture<'static, ()>) {
    schedule_smol(future);
}

#[cfg(feature = "async-std")]
pub fn schedule_async_std_boxed(future: BoxFuture<'static, ()>) {
    schedule_async_std(future);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_scheduler_blocks() {
        let scheduler = blocking_scheduler();
        use std::sync::{Arc, Mutex};

        let counter = Arc::new(Mutex::new(0));
        let inner = Arc::clone(&counter);

        scheduler.schedule(async move {
            *inner.lock().unwrap() += 1;
        });

        assert_eq!(*counter.lock().unwrap(), 1);
    }
}
