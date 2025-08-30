//! Executor module providing specialized effect execution strategies
//!
//! This module contains executor implementations for different execution models:
//! - TokioExecutor: Spawns each effect as a tokio task
//! - SingleCoreExecutor: Sequential execution on single thread
//! - RayonExecutor: Parallel execution using rayon work-stealing
//!
//! Executors use the same Storage pattern as models/resources for type-safe
//! multi-executor management.

pub mod storage;
pub mod tokio_executor;
pub mod single_thread_executor;
pub mod thread_per_core_tokio;
#[cfg(feature = "rayon")]
pub mod rayon_executor;

use crate::error::ShellError;
use crate::task::{TaskId, TaskStats};
use crate::timer::Time;
use std::future::Future;
use std::time::Duration;

// Re-export commonly used types
pub use storage::{ExecutorStorage, EmptyExecutorStorage};
pub use tokio_executor::TokioExecutor;
pub use single_thread_executor::SingleThreadExecutor;
pub use thread_per_core_tokio::ThreadPerCoreTokioExecutor;
#[cfg(feature = "rayon")]
pub use rayon_executor::RayonExecutor;

/// Accessor for an executor's resource storage
///
/// This trait enables higher-level helpers (like EffectContext::run_on_magic)
/// to construct an EffectContext backed by the executor's own resources.
pub trait HasExecutorResources<Event>: Send + Sync + 'static
where
    Event: Clone + Send + 'static,
{
    /// Resource storage type carried by this executor
    type Resources: Clone + Send + Sync + 'static;

    /// Clone the executor's resources for use in a new EffectContext
    fn clone_resources(&self) -> Self::Resources;
}

/// Legacy trait for effect executors (DEPRECATED)
///
/// This trait was used when executors managed effects directly.
/// New architecture: Shell distributes effects, executors provide spawning/runtime services.
///
/// # Type Parameters
/// - `Event`: Event type that can be sent back to Core
/// - `Effect`: Effect type this executor can handle
/// - `Resources`: Resources available to this executor
// Deprecated Executor trait removed - use SpawnExecutor and HasExecutorResources instead

/// Simplified executor trait focused on spawning and runtime services
/// 
/// Executors provide task spawning with safety guarantees and runtime services.
/// Shell handles effect distribution and calls executor methods directly.
///
/// # Type Parameters
/// - `Event`: Event type that can be sent back to Core
/// - `Resources`: Resources available to this executor
pub trait SpawnExecutor<Event, Resources>: Send + Sync + 'static
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Spawn a task with cleanup guarantees
    ///
    /// All tasks spawned through this method will be tracked and cleaned up
    /// when the executor is dropped.
    fn spawn<F>(&self, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static;

    /// Batch spawn multiple tasks efficiently
    fn spawn_batch<I>(&self, futures: I) -> Result<Vec<TaskId>, ShellError>
    where
        I: IntoIterator,
        I::Item: Future<Output = ()> + Send + 'static;

    /// Spawn a task with timeout
    fn spawn_with_timeout<F>(&self, duration: Duration, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static;

    /// Get runtime implementation for timeout operations
    fn runtime(&self) -> Time;

    /// Get immutable reference to a resource by type
    fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>;

    /// Get task statistics for monitoring
    fn task_stats(&self) -> TaskStats;

    /// Clean up finished tasks
    fn cleanup_finished_tasks(&self) -> (usize, usize);
}
