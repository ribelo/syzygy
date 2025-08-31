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

use crate::{
    error::ShellError,
    executor::{EmptyExecutorStorage, HasExecutorResources, SpawnExecutor},
    magic_handler::effect_trigger_direct,
    storage::EmptyStorage,
};
use futures::channel::oneshot;
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
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # use crossbeam_channel::unbounded;
    /// # #[derive(Debug, Clone)] enum MyEvent { Completed }
    /// # let (tx, _rx) = unbounded();
    /// # let ctx = EffectContext::<MyEvent, EmptyStorage>::new(Some(tx), EmptyStorage::new(), EmptyExecutorStorage::new());
    /// if let Some(sender) = ctx.event_sender() {
    ///     let _ = sender.send(MyEvent::Completed);
    /// }
    /// ```
    #[must_use]
    pub fn event_sender(&self) -> Option<Sender<Event>> {
        self.event_tx.clone()
    }

    /// Run a magic effect handler on a specific executor using that executor's
    /// resource storage, and await completion.
    ///
    /// This integrates with the magic handler system without requiring a split
    /// effect enum. You pass the concrete variant payload and a magic handler
    /// closure that declares its required resources and EventSender.
    pub async fn run_on<E, Index, Variant, ExecRes, Args, H>(
        &self,
        payload: Variant,
        handler: H,
    ) -> Result<(), ShellError>
    where
        Executors: crate::storage::Selector<E, Index>,
        E: SpawnExecutor<Event, ExecRes> + HasExecutorResources<Event, Resources = ExecRes>,
        ExecRes: Clone + Send + Sync + 'static,
        H: crate::magic_handler::EffectMagicHandlerDirect<Variant, Event, ExecRes, Args> + Send + 'static,
        Variant: Send + 'static,
        Event: Clone,
    {
        let exec: &E = self.executor::<E, Index>();
        let resources = exec.clone_resources();
        let event_tx = self.event_sender();

        // Build a context tailored to the executor's resources
        let exec_ctx = EffectContext::<Event, ExecRes>::new(event_tx, resources, EmptyExecutorStorage::new());

        let (tx, rx) = oneshot::channel::<()>();
        // Spawn the magic handler on the chosen executor
        exec.spawn(async move {
            let _ = effect_trigger_direct::<_, Event, ExecRes, _, _>(payload, exec_ctx, handler).await;
            let _ = tx.send(());
        })?;

        // Await completion to preserve sequential semantics when desired
        let _ = rx.await;
        Ok(())
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
