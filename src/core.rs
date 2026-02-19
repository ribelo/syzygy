//! # Core - Synchronous State Management
//!
//! This module provides the `Core` component, which is the heart of Syzygy's
//! synchronous state management. It is responsible for processing events,
//! updating the application's models, and generating commands for side effects.
//!
//! ## Key Components
//! - `Core` - The main struct that owns the application state (models) and processes events.
//! - `EventHandler` - A type alias for the function that contains the application's update logic.
//!
//! ## Example
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Clone)] enum TestEvent { Increment }
//! # #[derive(Debug, Clone)] enum TestEffect { Log }
//! # #[derive(Debug, Default)] struct CounterModel { count: i32 }
//! # fn counter_update(event: TestEvent, model: &mut CounterModel) -> Command<TestEvent, TestEffect> {
//! #     model.count += 1;
//! #     Command::none()
//! # }
//! // In a real application, you would build the core like this:
//! let model = CounterModel::default();
//! let (mut core, sender) = Core::new(counter_update, model);
//!
//! // Send an event to the core
//! sender.send(TestEvent::Increment).unwrap();
//!
//! // Process the event queue
//! let commands = core.process_events();
//!
//! // The model is now updated
//! let model_ref: &CounterModel = core.model();
//! assert_eq!(model_ref.count, 1);
//! ```
use std::collections::VecDeque;
use std::time::Duration;

use crossbeam_channel::{
    bounded, unbounded, Receiver as CoreReceiver, RecvTimeoutError as CoreRecvTimeoutError,
    Sender as CoreSender, TryRecvError as CoreTryRecvError,
};

type CoreSendError<E> = crossbeam_channel::SendError<E>;

#[cfg(feature = "tracing")]
use tracing::{debug, span, Level};

use crate::command::Command;
use crate::extract::EventContext;

pub(crate) type EventHandlerFn<E, X, M> = Box<dyn Fn(E, &EventContext<M>) -> Command<E, X> + Send>;

/// Multi-producer sender returned by [`Core::new`].
///
/// Wraps the underlying channel sender used by the Core.
pub struct EventSender<E> {
    inner: CoreSender<E>,
}

impl<E> Clone for EventSender<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E> EventSender<E> {
    fn new(inner: CoreSender<E>) -> Self {
        Self { inner }
    }

    /// Send an event to the core.
    pub fn send(&self, event: E) -> Result<(), CoreSendError<E>> {
        self.inner.send(event)
    }

    /// Alias for [`send`]. Kept for backward compatibility; on bounded channels this will
    /// still block when the queue is full.
    pub fn try_send(&self, event: E) -> Result<(), CoreSendError<E>> {
        self.send(event)
    }

    /// Attempt to send an event without blocking.
    ///
    /// Returns a [`CoreError::ChannelFull`](crate::error::CoreError::ChannelFull) when the queue
    /// is at capacity, allowing callers to implement backpressure strategies.
    pub fn try_send_event(&self, event: E) -> Result<(), crate::error::CoreError> {
        self.inner.try_send(event).map_err(Into::into)
    }
}

struct EventReceiver<E> {
    inner: CoreReceiver<E>,
}

impl<E> EventReceiver<E> {
    fn new(inner: CoreReceiver<E>) -> Self {
        Self { inner }
    }

    fn try_recv(&self) -> Result<E, CoreTryRecvError> {
        match self.inner.try_recv() {
            Ok(event) => Ok(event),
            Err(err) => Err(err),
        }
    }

    fn recv_timeout(&self, timeout: Duration) -> Result<E, CoreRecvTimeoutError> {
        match self.inner.recv_timeout(timeout) {
            Ok(event) => Ok(event),
            Err(err) => Err(err),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

fn event_channel<E>(capacity: Option<usize>) -> (CoreSender<E>, CoreReceiver<E>) {
    match capacity {
        Some(capacity) => bounded(capacity),
        None => unbounded(),
    }
}

/// Core handles synchronous event processing and owns the model.
///
/// Core is designed to be used on any thread, including UI threads, as it
/// never blocks and all operations are synchronous. It processes events,
/// updates the model, and returns Commands describing effects to execute.
pub struct Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// The update function that processes events
    event_handler: EventHandlerFn<E, X, M>,

    /// The model - owned and mutable
    model: M,

    /// Queue of events to process
    event_queue: VecDeque<E>,

    /// Pre-allocated command buffer to avoid reallocations
    command_buffer: Vec<Command<E, X>>,

    /// Channel for receiving external events
    event_rx: EventReceiver<E>,

    /// Channel for sending events (kept for cloning)
    event_tx: EventSender<E>,

    /// Configured inbound event capacity (None => unbounded)
    event_channel_capacity: Option<usize>,
}

impl<E, X, M> Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// Create a new Core with update function and storage
    pub fn new(event_handler: EventHandlerFn<E, X, M>, models: M) -> (Self, EventSender<E>) {
        Self::with_event_channel_capacity(event_handler, models, None)
    }

    /// Create a new Core with a specific inbound event channel capacity.
    pub fn with_event_channel_capacity(
        event_handler: EventHandlerFn<E, X, M>,
        models: M,
        capacity: Option<usize>,
    ) -> (Self, EventSender<E>) {
        let (raw_tx, raw_rx) = event_channel::<E>(capacity);
        let event_tx = EventSender::new(raw_tx);
        let event_rx = EventReceiver::new(raw_rx);

        let core = Self {
            event_handler,
            model: models,
            event_queue: VecDeque::with_capacity(16), // Pre-size for typical usage
            command_buffer: Vec::with_capacity(16),   // Pre-allocated command buffer
            event_rx,
            event_tx: event_tx.clone(),
            event_channel_capacity: capacity,
        };

        (core, event_tx)
    }

    /// Process a single event synchronously
    ///
    /// This is the main entry point for event processing. It updates the model
    /// and returns a Command describing any effects to execute.
    pub fn handle_event(&mut self, event: E) -> Command<E, X> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "handle_event").entered();

        #[cfg(feature = "tracing")]
        debug!("Processing event");

        let ctx = EventContext::new(&mut self.model);
        let command = (self.event_handler)(event, &ctx);

        #[cfg(feature = "tracing")]
        debug!("Event processed, command created");

        command
    }

    /// Process all events in the queue
    ///
    /// This processes all pending events and collects their commands.
    /// Returns a Vec<Command<E, X>> containing all commands generated.
    /// An empty vector indicates no events were processed.
    pub fn process_events(&mut self) -> Vec<Command<E, X>> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "process_events").entered();

        // Process events from external channel first
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_queue.push_back(event);
        }

        // Process all events in queue
        while let Some(event) = self.event_queue.pop_front() {
            let command = self.handle_event(event);
            self.command_buffer.push(command);
        }

        #[cfg(feature = "tracing")]
        if !self.command_buffer.is_empty() {
            debug!(events_processed = self.command_buffer.len());
        }

        let mut out = Vec::with_capacity(self.command_buffer.capacity().max(16));
        std::mem::swap(&mut self.command_buffer, &mut out);
        self.command_buffer.clear();
        out
    }

    /// Send an event to be processed in the next tick.
    #[deprecated(note = "use try_send_event() which returns Result")]
    pub fn send_event(&self, event: E) {
        let _ = self.try_send_event(event);
    }

    /// Attempt to send an event, returning an error if the channel is closed.
    pub fn try_send_event(&self, event: E) -> Result<(), crate::error::CoreError> {
        self.event_tx.inner.try_send(event).map_err(Into::into)
    }

    /// Get a sender for external events
    #[must_use]
    pub fn event_sender(&self) -> EventSender<E> {
        self.event_tx.clone()
    }

    /// Get the configured inbound event channel capacity (None => unbounded).
    #[must_use]
    pub fn event_channel_capacity(&self) -> Option<usize> {
        self.event_channel_capacity
    }

    /// Get immutable reference to the model
    #[must_use]
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Get mutable reference to the model
    ///
    /// This should be used carefully as it bypasses event processing.
    /// Prefer sending events for state changes.
    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    /// Get the current count of pending events.
    ///
    /// Returns a snapshot count of events waiting to be processed: queue + channel.
    /// This value can change immediately due to concurrent senders. Intended for
    /// metrics, logging, or heuristic backpressure decisions.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        let channel_count = self.event_rx.len();
        self.event_queue.len() + channel_count
    }

    /// Check if there are any pending events.
    ///
    /// Returns `true` if either the internal queue or inbound channel currently
    /// holds at least one event. This is a snapshot only; new events may arrive
    /// immediately after this returns. Use for control flow decisions like
    /// whether to continue ticking.
    #[must_use]
    pub fn has_pending_events(&self) -> bool {
        // Early-out optimization: check local queue first since it's cheaper
        !self.event_queue.is_empty() || !self.event_rx.is_empty()
    }

    /// Block until a new event arrives or the timeout expires.
    ///
    /// Returns true if an event was received and queued.
    pub fn wait_for_event(&mut self, timeout: Duration) -> bool {
        if timeout.is_zero() {
            return false;
        }

        match self.event_rx.recv_timeout(timeout) {
            Ok(event) => {
                self.event_queue.push_back(event);
                true
            }
            Err(CoreRecvTimeoutError::Timeout | CoreRecvTimeoutError::Disconnected) => false,
        }
    }
}

impl<E, X, M> std::fmt::Debug for Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
    M: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("model", &self.model)
            .field("pending_events", &!self.event_queue.is_empty())
            .field("command_buffer_size", &self.command_buffer.len())
            .finish_non_exhaustive()
    }
}
