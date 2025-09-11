//! # `EffectContext` - Container for executors and resources
//!
//! This module provides the `EffectContext`, a container that gives effect handlers
//! access to executors and resources. Executors handle task spawning and runtime
//! operations, while the context provides type-safe access to them.
//!
//! ## Key Components
//! - `EffectContext` - Container for executors and resources with event sending capability.
//!
//! ## Example
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Clone)] enum TestEvent { Done }
//! # #[derive(Debug, Clone)] enum TestEffect { PerformAsyncWork }
//! async fn handle_effects(effect: TestEffect, ctx: EffectContext<TestEvent, EmptyStorage>) -> syzygy::executor::Outcome<TestEvent> {
//!     if let TestEffect::PerformAsyncWork = effect {
//!         // Use direct async handling or return event output
//!         return syzygy::executor::Outcome::Event(TestEvent::Done);
//!     }
//!     syzygy::executor::Outcome::None
//! }
//! ```
//!
//! This provides clean separation of concerns:
//! - `EffectContext` holds executors and resources
//! - Executors handle spawning and provide runtime services
//! - Resources provide shared data access
//! - Event sending bridges back to the Core

use std::sync::Arc;

use crate::executor::{AsyncExecutor, ExecutorRegistry, SyncExecutor};
use std::any::TypeId;

/// `EffectContext` provides access to executors and resources within effect handlers
///
/// This context is a simple container that holds executors and resources,
/// allowing effect handlers to access them in a type-safe manner. Key features:
/// - Type-safe resource access via `resource()` method
/// - Type-safe executor access via `executor()` method
/// - Pure container - no spawning or runtime functionality
///
/// Executors handle all spawning and runtime operations.
/// Event sending is handled internally by the framework.
pub struct EffectContext<E, R> {
    /// Resources available to effect handlers (stored directly, user controls Arc/Mutex)
    resources: R,
    /// Executors available to effect handlers
    executors: Arc<ExecutorRegistry<E>>,
}

impl<E, R> EffectContext<E, R>
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    /// Create a new `EffectContext` (without event sending capability)
    #[must_use]
    pub fn new(resources: R, executors: Arc<ExecutorRegistry<E>>) -> Self {
        Self {
            resources,
            executors,
        }
    }

    /// Create a new `EffectContext` with explicit parts (without event sending)
    #[must_use]
    pub fn with_parts(resources: R, executors: Arc<ExecutorRegistry<E>>) -> Self {
        Self {
            resources,
            executors,
        }
    }

    /// Try to get a resource of type T
    pub fn resources(&self) -> &R {
        &self.resources
    }

    /// Get the executors registry (for internal use)
    pub(crate) fn executors(&self) -> &Arc<ExecutorRegistry<E>> {
        &self.executors
    }



    pub(crate) fn async_executor_by_typeid(
        &self,
        key: TypeId,
    ) -> Option<Arc<dyn AsyncExecutor<E>>> {
        self.executors.async_exec_by_key(key)
    }

    pub(crate) fn sync_executor_by_typeid(&self, key: TypeId) -> Option<Arc<dyn SyncExecutor<E>>> {
        self.executors.sync_exec_by_key(key)
    }

    pub fn async_executor<T: 'static>(&self) -> Option<Arc<dyn AsyncExecutor<E>>> {
        self.executors.async_exec::<T>()
    }

    pub fn sync_executor<T: 'static>(&self) -> Option<Arc<dyn SyncExecutor<E>>> {
        self.executors.sync_exec::<T>()
    }

    /// Spawn async work on specified executor
    pub fn spawn<Exec, F, Fut, O>(&self, f: F) -> crate::executor::spec::Task<E, R>
    where
        Exec: crate::executor::AsyncExecutor<E> + 'static,
        F: FnOnce(EffectContext<E, R>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = O> + Send + 'static,
        O: Into<crate::executor::Outcome<E>>,
        E: Send + 'static,
        R: Clone + Send + Sync + 'static,
    {
        crate::executor::spec::Task::future_on::<Exec, _, _, _>(f)
    }

    /// Spawn sync work on specified executor
    pub fn spawn_sync<Exec, F, O>(&self, f: F) -> crate::executor::spec::Task<E, R>
    where
        Exec: crate::executor::SyncExecutor<E> + 'static,
        F: FnOnce(EffectContext<E, R>) -> O + Send + 'static,
        O: Into<crate::executor::Outcome<E>>,
        E: Send + 'static,
        R: Clone + Send + Sync + 'static,
    {
        crate::executor::spec::Task::sync_on::<Exec, _, _>(f)
    }

    /// Spawn a stream on specified executor
    pub fn spawn_stream<Exec, F, S>(&self, f: F) -> crate::executor::spec::Task<E, R>
    where
        Exec: crate::executor::AsyncExecutor<E> + 'static,
        F: FnOnce(EffectContext<E, R>) -> S + Send + 'static,
        S: futures::Stream<Item = E> + Send + 'static,
        E: Send + 'static,
        R: Clone + Send + Sync + 'static,
    {
        crate::executor::spec::Task::stream_on::<Exec, _, _>(f)
    }
}

impl<E, R> Clone for EffectContext<E, R>
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
            executors: Arc::clone(&self.executors),
        }
    }
}

// Tests removed - storage system deleted
