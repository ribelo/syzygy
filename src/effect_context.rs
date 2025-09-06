//! # EffectContext - Container for executors and resources
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
//! - EffectContext holds executors and resources
//! - Executors handle spawning and provide runtime services
//! - Resources provide shared data access
//! - Event sending bridges back to the Core

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use crossbeam_channel::Sender;
use rustc_hash::FxHashMap;

/// EffectContext provides access to executors and resources within effect handlers
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
    executors: FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    /// Channel for sending events
    event_tx: Sender<E>,
}

impl<E, R> EffectContext<E, R>
where
    E: Clone + Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    /// Create a new EffectContext
    #[must_use]
    pub fn new(
        resources: R,
        event_tx: Sender<E>,
        executors: FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    ) -> Self {
        Self {
            resources,
            executors,
        }
    }

    /// Try to get a resource of type T
    pub fn try_resource<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        // Since storage system was removed, this is a placeholder implementation
        // In a real implementation, this would access the resources storage
        None
    }

    /// Get a resource of type T, panicking if not found
    pub fn resource<T>(&self) -> &T
    where
        T: 'static,
    {
        // Since storage system was removed, this is a placeholder implementation
        // In a real implementation, this would access the resources storage
        panic!("Resource of type {} not found", std::any::type_name::<T>())
    }
}

impl<Resources> Clone for EffectContext<Resources>
where
    Resources: Clone,
{
    fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
            executors: self.executors.clone(),
        }
    }
}

// Tests removed - storage system deleted
