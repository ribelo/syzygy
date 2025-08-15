//! Builder for THE Syzygy system with single model
//!
//! This builder creates the unified Syzygy system with a single model
//! and simple resource management for great developer experience.

use crate::handle::SyzygyHandle;
use crate::dispatch::Dispatch;
use crate::context::CommandContext;
use crate::resource::Resources;
use crossbeam_channel::{bounded, unbounded};
use std::marker::PhantomData;
use std::future::Future;
use std::pin::Pin;

#[cfg(feature = "tracing")]
use tracing::debug;

// ============================================================================
// Builder State Types
// ============================================================================

/// Initial stage - can configure model and resources
pub struct InitialStage;

// ============================================================================
// SyzygyBuilder - Simple builder with single model
// ============================================================================

/// Builder for Syzygy systems with single model and resources
pub struct SyzygyBuilder<M, E, C, R = Resources, S = InitialStage>
where
    E: Clone,
{
    model: Option<M>,
    resources: R,
    event_buffer_size: Option<usize>,
    command_buffer_size: Option<usize>,
    event_handler: Option<fn(E, &mut M) -> Dispatch<E, C>>,
    command_handler: Option<fn(CommandContext<E, R>, C) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    _phantom: PhantomData<S>,
}

impl<Event, Command> SyzygyBuilder<(), Event, Command, Resources, InitialStage>
where
    Event: Send + Clone + 'static,
    Command: Send + Clone + 'static,
{
    /// Create a new typed builder in the initial InitialStage
    pub(crate) fn new() -> Self {
        Self {
            model: None,
            resources: Resources::default(),
            event_buffer_size: None,
            command_buffer_size: None,
            event_handler: None,
            command_handler: None,
            _phantom: PhantomData,
        }
    }
}

impl<M, Event, Command, R> SyzygyBuilder<M, Event, Command, R, InitialStage>
where
    Event: Send + Clone + 'static,
    Command: Send + Clone + 'static,
{
    /// Set the event channel buffer size
    pub fn event_buffer_size(mut self, size: usize) -> Self {
        self.event_buffer_size = Some(size);
        self
    }

    /// Set the command channel buffer size
    pub fn command_buffer_size(mut self, size: usize) -> Self {
        self.command_buffer_size = Some(size);
        self
    }
}

impl<Event, Command> SyzygyBuilder<(), Event, Command, Resources, InitialStage>
where
    Event: Send + Clone + 'static,
    Command: Send + Clone + 'static,
{
    /// Set the model
    pub fn model<M>(self, model: M) -> SyzygyBuilder<M, Event, Command, Resources, InitialStage> {
        SyzygyBuilder {
            model: Some(model),
            resources: self.resources,
            event_buffer_size: self.event_buffer_size,
            command_buffer_size: self.command_buffer_size,
            event_handler: None,
            command_handler: None,
            _phantom: PhantomData,
        }
    }

    /// Add a resource
    pub fn resource<R>(self, resource: R) -> Self
    where
        R: Send + Sync + 'static,
    {
        let resources = {
            let r = self.resources;
            r.insert(resource);
            r
        };
        Self {
            model: self.model,
            resources,
            event_buffer_size: self.event_buffer_size,
            command_buffer_size: self.command_buffer_size,
            event_handler: self.event_handler,
            command_handler: self.command_handler,
            _phantom: self._phantom,
        }
    }
}

impl<M, E, C> SyzygyBuilder<M, E, C, Resources, InitialStage>
where
    E: Send + Clone + 'static,
    C: Send + Clone + 'static,
{
    /// Set the event handler - a function that processes the model state
    ///
    /// The handler receives only the mutable model and returns a Dispatch.
    /// Must be a function pointer (not a closure with captures).
    pub fn event_handler(mut self, handler: fn(E, &mut M) -> Dispatch<E, C>) -> Self {
        self.event_handler = Some(handler);
        self
    }

    /// Set the command handler for async command execution (optional)
    ///
    /// The handler receives a CommandContext and command, and returns a boxed Future.
    /// Must be a function pointer (not a closure with captures).
    pub fn command_handler(mut self, handler: fn(CommandContext<E, Resources>, C) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> Self {
        self.command_handler = Some(handler);
        self
    }

    /// Build the Syzygy system
    ///
    /// Requires model and event handler to be set.
    /// If a command handler is set, also returns a CommandExecutor.
    #[cfg(feature = "async")]
    pub fn build(
        self,
    ) -> (
        crate::syzygy::Syzygy<M, E, C, Resources>,
        SyzygyHandle<E>,
        Option<crate::executor::CommandExecutor<E, C>>,
    ) {
        let model = self.model.expect("Model must be set before building");
        let event_handler = self.event_handler.expect("Event handler must be set before building");

        // Create channels
        let (event_tx, event_rx) = match self.event_buffer_size {
            Some(size) => bounded(size),
            None => unbounded(),
        };

        let (command_tx, command_rx) = match self.command_buffer_size {
            Some(size) => bounded(size),
            None => unbounded(),
        };

        // Create executor if command handler is provided
        let executor = if let Some(command_handler) = self.command_handler {
            Some(crate::executor::CommandExecutor::new(
                command_rx,
                event_tx.clone(),
                self.resources.clone(),
                command_handler,
            ))
        } else {
            None
        };

        let syzygy = crate::syzygy::Syzygy {
            model,
            resources: self.resources,
            event_rx,
            event_tx: event_tx.clone(),
            command_tx: command_tx.clone(),
            event_handler,
            command_handler: self.command_handler,
            processed_events_count: 0,
        };

        let handle = SyzygyHandle::new(event_tx);

        #[cfg(feature = "tracing")]
        debug!("Built Syzygy with single model dispatch");

        (syzygy, handle, executor)
    }

    /// Build the Syzygy system (without async features)
    ///
    /// Requires model and event handler to be set.
    #[cfg(not(feature = "async"))]
    pub fn build(
        self,
    ) -> (
        crate::syzygy::Syzygy<M, E, C, Resources>,
        SyzygyHandle<E>,
    ) {
        let model = self.model.expect("Model must be set before building");
        let event_handler = self.event_handler.expect("Event handler must be set before building");

        // Create channels
        let (event_tx, event_rx) = match self.event_buffer_size {
            Some(size) => bounded(size),
            None => unbounded(),
        };

        let (command_tx, _command_rx) = match self.command_buffer_size {
            Some(size) => bounded(size),
            None => unbounded(),
        };

        let syzygy = crate::syzygy::Syzygy {
            model,
            resources: self.resources,
            event_rx,
            event_tx: event_tx.clone(),
            command_tx: command_tx.clone(),
            event_handler,
            command_handler: self.command_handler,
            processed_events_count: 0,
        };

        let handle = SyzygyHandle::new(event_tx);

        #[cfg(feature = "tracing")]
        debug!("Built Syzygy with single model dispatch");

        (syzygy, handle)
    }

}
