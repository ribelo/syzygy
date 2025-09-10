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
//! async fn handle_effects(effect: TestEffect, ctx: EffectContext<TestEvent, EmptyStorage>) -> syzygy::streaming::EffectOutput<TestEvent> {
//!     if let TestEffect::PerformAsyncWork = effect {
//!         // Use direct async handling or return event output
//!         return syzygy::streaming::EffectOutput::Single(TestEvent::Done);
//!     }
//!     syzygy::streaming::EffectOutput::None
//! }
//! ```
//!
//! This provides clean separation of concerns:
//! - `EffectContext` holds executors and resources
//! - Executors handle spawning and provide runtime services
//! - Resources provide shared data access
//! - Event sending bridges back to the Core

use std::sync::Arc;

use crossbeam_channel::Sender;
// use rustc_hash::FxHashMap; // legacy

use crate::executor::{AsyncExecutor, ExecutorRegistry, SyncExecutor};
use std::any::TypeId;

/// `EffectContext` provides access to executors and resources within effect handlers
///
/// This context is a simple container that holds executors and resources,
/// allowing effect handlers to access them in a type-safe manner. Key features:
/// - Type-safe resource access via `resource()` method
/// - Type-safe executor access via `executor()` method
/// - Event sending capability to communicate back to Core
/// - Pure container - no spawning or runtime functionality
///
/// Executors handle all spawning and runtime operations.
pub struct EffectContext<E, R> {
    /// Resources available to effect handlers (stored directly, user controls Arc/Mutex)
    resources: R,
    /// Executors available to effect handlers
    executors: Arc<ExecutorRegistry<E>>,
    /// Channel for sending events
    event_tx: Sender<E>,
}

impl<E, R> EffectContext<E, R>
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    /// Create a new `EffectContext`
    #[must_use]
    /// Legacy constructor: `event_tx` first to match older call sites in Shell
    pub fn new(event_tx: Sender<E>, resources: R, executors: Arc<ExecutorRegistry<E>>) -> Self {
        Self {
            resources,
            executors,
            event_tx,
        }
    }

    /// Preferred constructor: resources first for consistency with other builders
    pub fn with_parts(
        resources: R,
        event_tx: Sender<E>,
        executors: Arc<ExecutorRegistry<E>>,
    ) -> Self {
        Self {
            resources,
            executors,
            event_tx,
        }
    }

    /// Try to get a resource of type T
    pub fn resources(&self) -> &R {
        &self.resources
    }

    /// Send an event back to Core. Panics if the event channel is closed.
    pub fn send_event(&self, ev: E) {
        self.event_tx
            .send(ev)
            .expect("event channel is closed; Core should own the receiver while running");
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
            event_tx: self.event_tx.clone(),
        }
    }
}

// Tests removed - storage system deleted
