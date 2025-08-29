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
//! async fn handle_effects(effect: TestEffect, ctx: EffectContext<TestEvent, (), ()>) {
//!     if let TestEffect::PerformAsyncWork = effect {
//!         // Get executor and use it to spawn tasks
//!         let executor = ctx.executor::<MyExecutor>();
//!         executor.spawn(async move {
//!             // Work happens here
//!             let _ = ctx.send_event(TestEvent::Done);
//!         }).await;
//!     }
//! }
//! ```
//!
//! This provides clean separation of concerns:
//! - EffectContext holds executors and resources
//! - Executors handle spawning and provide runtime services
//! - Resources provide shared data access
//! - Event sending bridges back to the Core

use crate::{error::ShellError, executor::EmptyExecutorStorage, storage::EmptyStorage};
use crossbeam_channel::Sender;

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
pub struct EffectContext<Event, Resources = EmptyStorage, Executors = EmptyExecutorStorage> {
    /// Channel to send events back to Core
    event_tx: Option<Sender<Event>>,
    /// Resources available to effect handlers (stored directly, user controls Arc/Mutex)
    resources: Resources,
    /// Executors available to effect handlers
    executors: Executors,
}

impl<Event, Resources, Executors> EffectContext<Event, Resources, Executors>
where
    Event: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
    Executors: Clone + Send + Sync + 'static,
{
    /// Create a new EffectContext
    #[must_use]
    pub fn new(
        event_tx: Option<Sender<Event>>,
        resources: Resources,
        executors: Executors,
    ) -> Self {
        Self {
            event_tx,
            resources,
            executors,
        }
    }

    /// Send an event back to the Core
    pub fn send_event(&self, event: Event) -> Result<(), ShellError> {
        if let Some(ref tx) = self.event_tx {
            tx.send(event).map_err(|_| ShellError::EventChannelClosed)?;
            Ok(())
        } else {
            Err(ShellError::EventChannelClosed)
        }
    }

    /// Get immutable reference to a specific resource by type
    ///
    /// This is a convenience method that delegates to the resources' get() method.
    /// The type must exist in the resources chain for this to compile.
    ///
    /// # Example
    /// ```rust,ignore
    /// let client: &HttpClient = ctx.resource();
    /// // Or with explicit type:
    /// let client = ctx.resource::<HttpClient>();
    /// ```
    #[must_use]
    pub fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>,
    {
        self.resources.get()
    }

    /// Get immutable reference to a specific executor by type
    ///
    /// This is a convenience method that delegates to the executors' get() method.
    /// The type must exist in the executors chain for this to compile.
    ///
    /// # Example
    /// ```rust,ignore
    /// let executor: &TokioExecutor<Event, Effect> = ctx.executor();
    /// // Or with explicit type:
    /// let executor = ctx.executor::<TokioExecutor<Event, Effect>>();
    /// ```
    #[must_use]
    pub fn executor<T, Index>(&self) -> &T
    where
        Executors: crate::storage::Selector<T, Index>,
    {
        self.executors.get()
    }

    /// Get a clone of the event sender if available
    ///
    /// This is useful for magic handlers that need to send events
    /// but don't need the full EffectContext.
    ///
    /// # Example
    /// ```rust,ignore
    /// if let Some(sender) = ctx.event_sender() {
    ///     sender.send(MyEvent::Completed);
    /// }
    /// ```
    #[must_use]
    pub fn event_sender(&self) -> Option<Sender<Event>> {
        self.event_tx.clone()
    }
}

impl<Event, Resources, Executors> Clone for EffectContext<Event, Resources, Executors>
where
    Resources: Clone,
    Executors: Clone,
{
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone(), // User-controlled clone semantics
            executors: self.executors.clone(), // User-controlled clone semantics
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        use crate::executor::EmptyExecutorStorage;
        use crate::storage::EmptyStorage;

        // Test that context creation is fast and simple
        let resources = EmptyStorage::new();
        let executors = EmptyExecutorStorage::default();
        let _ctx: EffectContext<()> = EffectContext::new(None, resources, executors);
    }

    #[test]
    fn test_context_clone() {
        use crate::executor::EmptyExecutorStorage;
        use crate::storage::EmptyStorage;

        let resources = EmptyStorage::new();
        let executors = EmptyExecutorStorage::default();
        let ctx: EffectContext<()> = EffectContext::new(None, resources, executors);

        let _cloned = ctx.clone();
    }

    #[test]
    fn test_event_sending() {
        use crate::executor::EmptyExecutorStorage;
        use crate::storage::EmptyStorage;
        use crossbeam_channel;

        let (tx, rx) = crossbeam_channel::unbounded();
        let resources = EmptyStorage::new();
        let executors = EmptyExecutorStorage::default();
        let ctx = EffectContext::new(Some(tx), resources, executors);

        // Send an event
        ctx.send_event(42).unwrap();

        // Verify it was received
        let received = rx.recv().unwrap();
        assert_eq!(received, 42);
    }
}
