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

use crate::async_context::EffectContext;
use crate::error::ShellError;
use crate::spawn::Spawn;
use crate::task::{TaskId, TaskStats};
use crate::timer::Time;
use std::future::Future;
use std::time::Duration;

// Re-export commonly used types
pub use storage::{ExecutorStorage, EmptyExecutorStorage};
pub use tokio_executor::TokioExecutor;

/// Legacy trait for effect executors (DEPRECATED)
///
/// This trait was used when executors managed effects directly.
/// New architecture: Shell distributes effects, executors provide spawning/runtime services.
///
/// # Type Parameters
/// - `Event`: Event type that can be sent back to Core
/// - `Effect`: Effect type this executor can handle
/// - `Resources`: Resources available to this executor
#[deprecated = "Use simplified executor pattern - Shell handles effects, executors provide spawning"]
pub trait Executor<Event, Effect, Resources>: Send + Sync + 'static
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Process queued effects for this executor
    fn tick<S>(&mut self, spawner: &S) -> Result<bool, ShellError>
    where
        S: Spawn;

    /// Handle a specific effect using the provided handler function
    fn handle<H>(&self, effect: Effect, handler: H)
    where
        H: FnOnce(Effect, EffectContext<Event, Resources>) + Send + 'static;

    /// Get immutable reference to a resource by type
    fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>;

    /// Check if executor has work queued
    fn has_work(&self) -> bool;
}

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