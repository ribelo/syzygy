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
//! # use syzygy::event_context::EventContext;
//! # #[derive(Debug, Clone)] enum TestEvent { Increment }
//! # #[derive(Debug, Clone)] enum TestEffect { Log }
//! # #[derive(Debug, Default)] struct CounterModel { count: i32 }
//! # fn counter_update(event: TestEvent, ctx: &mut EventContext<TestEvent, TestEffect, CounterModel>) -> Command<TestEvent, TestEffect> {
//! #     let model: &mut CounterModel = ctx.model_mut();
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
//! core.process_events();
//!
//! // The model is now updated
//! let model_ref: &CounterModel = core.model();
//! assert_eq!(model_ref.count, 1);
//! ```
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::VecDeque;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span};

use crate::{command::Command, event_context::EventContext};

/// Update function type that takes an event and a mutable `EventContext`
pub type EventHandler<E, X, M> = fn(event: E, ctx: &mut EventContext<E, X, M>) -> Command<E, X>;

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
    event_handler: EventHandler<E, X, M>,

    /// The model - owned and mutable
    model: M,

    /// Queue of events to process
    event_queue: VecDeque<E>,

    /// Pre-allocated command buffer to avoid reallocations
    command_buffer: Vec<Command<E, X>>,

    /// Channel for receiving external events
    event_rx: Receiver<E>,

    /// Channel for sending events (kept for cloning)
    event_tx: Sender<E>,
}

impl<E, X, M> Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// Create a new Core with update function and storage
    pub fn new(event_handler: EventHandler<E, X, M>, models: M) -> (Self, Sender<E>) {
        let (event_tx, event_rx) = unbounded();

        let core = Self {
            event_handler,
            model: models,
            event_queue: VecDeque::with_capacity(16), // Pre-size for typical usage
            command_buffer: Vec::with_capacity(16),   // Pre-allocated command buffer
            event_rx,
            event_tx: event_tx.clone(),
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

        let mut ctx = EventContext::new(&mut self.model);
        let command = (self.event_handler)(event, &mut ctx);

        #[cfg(feature = "tracing")]
        debug!("Event processed, command created");

        command
    }

    /// Process all events in the queue
    ///
    /// This processes all pending events and collects their commands.
    /// Returns true if any events were processed, false if queue was empty.
    pub fn process_events(&mut self) -> (bool, Vec<Command<E, X>>) {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "process_events").entered();

        let mut processed_any = false;

        // Process events from external channel first
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_queue.push_back(event);
        }

        // Process all events in queue
        while let Some(event) = self.event_queue.pop_front() {
            processed_any = true;

            let command = self.handle_event(event);
            self.command_buffer.push(command);
        }

        #[cfg(feature = "tracing")]
        if processed_any {
            debug!(events_processed = self.command_buffer.len());
        }

        (processed_any, std::mem::take(&mut self.command_buffer))
    }

    /// Send an event to be processed in the next tick.
    ///
    /// This expects the Core is alive and will panic if the event channel is closed.
    pub fn send_event(&self, event: E) {
        self.event_tx
            .send(event)
            .expect("event channel is closed; Runner/Core should own the receiver while running");
    }

    /// Get a sender for external events
    #[must_use]
    pub fn event_sender(&self) -> Sender<E> {
        self.event_tx.clone()
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
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
        Decrement,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    #[derive(Debug, Default)]
    struct CounterModel {
        count: i32,
    }

    fn counter_update(
        event: TestEvent,
        ctx: &mut EventContext<TestEvent, TestEffect, CounterModel>,
    ) -> Command<TestEvent, TestEffect> {
        let model: &mut CounterModel = ctx.model_mut();

        match event {
            TestEvent::Increment => {
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
            TestEvent::Decrement => {
                model.count -= 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[test]
    fn test_core_basic_functionality() {
        let model = CounterModel { count: 0 };

        let (mut core, _) = Core::new(counter_update, model);

        // Test event handling
        let _command = core.handle_event(TestEvent::Increment);

        // Verify model was updated
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 1);
    }

    #[test]
    fn test_core_event_processing() {
        let model = CounterModel { count: 0 };

        let (mut core, sender) = Core::new(counter_update, model);

        // Send events
        sender.send(TestEvent::Increment).unwrap();
        sender.send(TestEvent::Increment).unwrap();
        sender.send(TestEvent::Decrement).unwrap();

        // Process all events
        let (processed, commands) = core.process_events();

        assert!(processed);
        assert_eq!(commands.len(), 3);

        // Verify final model state
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 1); // +1 +1 -1 = 1
    }

    #[test]
    fn test_core_pending_events() {
        let model = CounterModel { count: 0 };

        let (mut core, sender) = Core::new(counter_update, model);

        assert_eq!(core.pending_count(), 0);
        assert!(!core.has_pending_events());

        sender.send(TestEvent::Increment).unwrap();
        assert!(core.pending_count() > 0);
        assert!(core.has_pending_events());

        core.process_events();
        assert_eq!(core.pending_count(), 0);
        assert!(!core.has_pending_events());
    }

    #[test]
    fn test_core_model_access() {
        let model = CounterModel { count: 42 };
        let (mut core, _) = Core::new(counter_update, model);

        // Test immutable model access
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 42);

        // Test mutable model access
        {
            let model_mut: &mut CounterModel = core.model_mut();
            model_mut.count = 100;
        }

        // Verify the mutation worked
        let model_after: &CounterModel = core.model();
        assert_eq!(model_after.count, 100);
    }

    #[test]
    fn test_core_multiple_models() {
        #[derive(Debug, Default)]
        struct UserModel {
            name: String,
            age: u32,
        }

        #[derive(Debug, Default)]
        struct ConfigModel {
            theme: String,
            debug: bool,
        }

        #[derive(Debug)]
        struct AppModel {
            counter: CounterModel,
            user: UserModel,
            config: ConfigModel,
        }

        // Update function for the multi-model app
        fn multi_model_update(
            event: TestEvent,
            ctx: &mut EventContext<TestEvent, TestEffect, AppModel>,
        ) -> Command<TestEvent, TestEffect> {
            // Just update the counter for simplicity
            let model = ctx.model_mut();
            match event {
                TestEvent::Increment => {
                    model.counter.count += 1;
                    Command::effect(TestEffect::Log)
                }
                TestEvent::Decrement => {
                    model.counter.count -= 1;
                    Command::effect(TestEffect::Log)
                }
            }
        }

        // Create app model with multiple sub-models
        let app_model = AppModel {
            counter: CounterModel { count: 10 },
            user: UserModel {
                name: "Alice".to_string(),
                age: 25,
            },
            config: ConfigModel {
                theme: "dark".to_string(),
                debug: true,
            },
        };

        let (mut core, _) = Core::new(multi_model_update, app_model);

        // Test accessing different models
        let model = core.model();
        assert_eq!(model.counter.count, 10);
        assert_eq!(model.user.name, "Alice");
        assert_eq!(model.user.age, 25);
        assert_eq!(model.config.theme, "dark");
        assert!(model.config.debug);

        // Test mutable access to different models
        {
            let model_mut = core.model_mut();
            model_mut.counter.count = 999;
            model_mut.user.name = "Bob".to_string();
            model_mut.user.age = 30;
            model_mut.config.theme = "light".to_string();
            model_mut.config.debug = false;
        }

        // Verify all mutations worked
        let model_after = core.model();
        assert_eq!(model_after.counter.count, 999);
        assert_eq!(model_after.user.name, "Bob");
        assert_eq!(model_after.user.age, 30);
        assert_eq!(model_after.config.theme, "light");
        assert!(!model_after.config.debug);
    }

    #[test]
    fn test_core_model_access_api() {
        let model = CounterModel { count: 0 };
        let (mut core, _) = Core::new(counter_update, model);

        // Test model access
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 0);

        // Mutate via model API
        {
            let model_mut: &mut CounterModel = core.model_mut();
            model_mut.count = 123;
        }

        // Verify mutation worked
        let model_after: &CounterModel = core.model();
        assert_eq!(model_after.count, 123);
    }
}
