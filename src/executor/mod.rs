//! Executors — specialized async and blocking runtimes.

use std::any::{Any, TypeId};
use std::time::Duration;

use futures_util::future::BoxFuture;
use thiserror::Error;

/// Type alias for the complex job function type used by ResourceBlockingExecutor
type ResourceJobFn = Box<dyn FnOnce(&mut dyn Any) + Send>;

#[cfg(feature = "rt-inline")]
pub mod inline_async;
pub mod registry;
pub mod task;

#[cfg(feature = "rt-inline")]
pub use inline_async::InlineAsync;
pub use registry::ExecutorRegistry;
pub use task::{PanicDetails, PanicHook, PanicTaskKind, Task};
/// Friendly alias for `Task` used in docs to highlight declarative plans.
pub type Plan<E, X> = Task<E, X>;

/// Error type returned when executors fail to schedule jobs.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("executor has been shut down")]
    Shutdown,
    #[error("executor worker is gone")]
    WorkerGone,
    #[error("task was cancelled")]
    Cancelled,
    #[error("panic: {msg}")]
    Panic { msg: String },
}

#[must_use]
pub fn panic_message(panic_payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic_payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = panic_payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unknown internal error".to_string()
    }
}

/// Shared lifecycle management for all executor types.
pub trait ExecutorLifecycle: Send + 'static {
    fn shutdown(&self);
    fn wait(&self);
}

/// Executor specialized for async work (futures).
pub trait AsyncExecutor: ExecutorLifecycle + Sync {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError>;

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// Executor for blocking work without shared resources.
pub trait BlockingExecutor: ExecutorLifecycle + Sync {
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError>;
}

/// Executor for blocking work with a dedicated, mutable resource.
pub trait ResourceBlockingExecutor: ExecutorLifecycle + Sync {
    fn resource_type_id(&self) -> TypeId;

    fn spawn_blocking_with_resource(&self, job: ResourceJobFn) -> Result<(), ExecutorError>;
}
