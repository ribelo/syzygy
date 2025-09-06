//! Executors — pluggable async runtimes for side effects
//!
//! Syzygy models are pure, but effects run on executors. This module
//! defines a small, pluggable `SpawnExecutor` trait and implementations for
//! different strategies (Tokio-based, single-threaded, etc.).
//!
//! Refactor note:
//! - TaskTracker has been removed from the executor API. Executors own their
//!   tasks and ensure correct shutdown semantics (drop cancels, explicit
//!   `shutdown`/`join` available) — following the dedicated executor pattern
//!   from influxdb3_core.

// Executor storage module removed - using direct FxHashMap
#[cfg(feature = "rayon")]
pub mod rayon_executor;
pub mod single_thread_executor;
pub mod thread_per_core_tokio;
pub mod tokio_executor;

use futures_util::future::BoxFuture;
use std::future::Future;

use thiserror::Error;
// Executor storage types removed
#[cfg(feature = "rayon")]
pub use rayon_executor::RayonExecutor;
pub use single_thread_executor::SingleThreadExecutor;
pub use thread_per_core_tokio::ThreadPerCoreTokioExecutor;
pub use tokio_executor::TokioExecutor;

use crate::prelude::{EffectContext, EffectResult};

/// Error type for executor job management
#[derive(Debug, Error)]
pub enum ExecutorError {
    /// The executor has been shut down and cannot accept new work
    #[error("Worker thread gone, executor was likely shut down")]
    WorkerGone,
    /// The spawned task panicked; contains message if available
    #[error("Panic: {msg}")]
    Panic { msg: String },
}

/// Minimal, pluggable executor API following the DedicatedExecutor pattern
///
/// - Owns a runtime/thread(s) and ensures tasks are cancelled when the
///   executor is dropped or explicitly shut down
/// - `spawn` returns a cancel-on-drop future that resolves to the task's
///   output or an `ExecError`
pub trait Executor<E, R>: Send + Sync + 'static {
    /// Spawn a task on this executor and get a cancel-on-drop join future
    fn spawn<F, Fut>(&self, task: F) -> BoxFuture<'static, Result<Fut::Output, ExecutorError>>
    where
        F: Fn(EffectContext<E, R>) -> Fut,
        Fut: Future<Ouptut = EffectResult<E>> + Send + 'static;

    /// Signal the executor to begin shutdown; no further tasks will be accepted
    fn shutdown(&self);

    /// Wait for executor shutdown completion
    fn join(&self) -> BoxFuture<'static, ()>;
}
