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
//! # use syzygy::storage::{EmptyStorage, Storage, StorageBuilder};
//! # #[derive(Debug, Clone)] enum TestEvent { Increment }
//! # #[derive(Debug, Clone)] enum TestEffect { Log }
//! # #[derive(Debug, Default)] struct CounterModel { count: i32 }
//! # fn counter_update(event: TestEvent, ctx: &mut EventContext<TestEvent, TestEffect, Storage<CounterModel, EmptyStorage>>) -> Command<TestEvent, TestEffect> {
//! #     let model: &mut CounterModel = ctx.model_mut();
//! #     model.count += 1;
//! #     Command::none()
//! # }
//! // In a real application, you would build the core like this:
//! let storage = EmptyStorage::new().with_model(CounterModel::default());
//! let (mut core, sender) = Core::new(counter_update, storage);
//!
//! // Send an event to the core
//! sender.send(TestEvent::Increment).unwrap();
//!
//! // Process the event queue
//! core.process_events();
//!
//! // The model is now updated
//! let model: &CounterModel = core.model();
//! assert_eq!(model.count, 1);
//! ```
use crate::{command::Command, event_context::EventContext, storage::Selector};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::VecDeque;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span};

/// Update function type that takes an event and a mutable EventContext
pub type EventHandler<Event, Effect, Storage> =
    fn(event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Command<Event, Effect>;

/// Core handles synchronous event processing and owns the model storage.
///
/// Core is designed to be used on any thread, including UI threads, as it
/// never blocks and all operations are synchronous. It processes events,
/// updates the models through storage, and returns Commands describing effects to execute.
pub struct Core<Event, Effect, Storage>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    /// The update function that processes events
    update_fn: EventHandler<Event, Effect, Storage>,

    /// The storage containing all models - owned and mutable
    models: Storage,

    /// Queue of events to process
    event_queue: VecDeque<Event>,

    /// Pre-allocated command buffer to avoid reallocations
    command_buffer: Vec<Command<Event, Effect>>,

    /// Channel for receiving external events
    event_rx: Receiver<Event>,

    /// Channel for sending events (kept for cloning)
    event_tx: Sender<Event>,
}

impl<Event, Effect, Storage> Core<Event, Effect, Storage>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    /// Create a new Core with update function and storage
    pub fn new(
        update_fn: EventHandler<Event, Effect, Storage>,
        models: Storage,
    ) -> (Self, Sender<Event>) {
        let (event_tx, event_rx) = unbounded();

        let core = Self {
            update_fn,
            models,
            event_queue: VecDeque::with_capacity(16), // Pre-size for typical usage
            command_buffer: Vec::with_capacity(16),   // Pre-allocated command buffer
            event_rx,
            event_tx: event_tx.clone(),
        };

        (core, event_tx)
    }

    /// Process a single event synchronously
    ///
    /// This is the main entry point for event processing. It updates the storage
    /// and returns a Command describing any effects to execute.
    pub fn handle_event(&mut self, event: Event) -> Command<Event, Effect> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "handle_event").entered();

        #[cfg(feature = "tracing")]
        debug!("Processing event");

        let mut ctx = EventContext::new(&mut self.models);
        let command = (self.update_fn)(event, &mut ctx);

        #[cfg(feature = "tracing")]
        debug!("Event processed, command created");

        command
    }

    /// Process all events in the queue
    ///
    /// This processes all pending events and collects their commands.
    /// Returns true if any events were processed, false if queue was empty.
    pub fn process_events(&mut self) -> (bool, Vec<Command<Event, Effect>>) {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "process_events").entered();

        self.command_buffer.clear();
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

        (processed_any, self.command_buffer.clone())
    }

    /// Send an event to be processed in the next tick
    pub fn send_event(&self, event: Event) -> Result<(), crossbeam_channel::SendError<Event>> {
        self.event_tx.send(event)
    }

    /// Get a sender for external events
    #[must_use]
    pub fn event_sender(&self) -> Sender<Event> {
        self.event_tx.clone()
    }

    /// Get immutable reference to the storage
    #[must_use]
    pub fn storage(&self) -> &Storage {
        &self.models
    }

    /// Get mutable reference to the storage
    ///
    /// This should be used carefully as it bypasses event processing.
    /// Prefer sending events for state changes.
    pub fn storage_mut(&mut self) -> &mut Storage {
        &mut self.models
    }

    /// Get an immutable reference to a specific model by type
    ///
    /// This is a convenience method that delegates to the storage's get() method.
    /// The type must exist in the storage chain for this to compile.
    ///
    #[must_use]
    pub fn model<T, Index>(&self) -> &T
    where
        Storage: Selector<T, Index>,
    {
        self.models.get()
    }

    /// Get a mutable reference to a specific model by type
    ///
    /// This is a convenience method that delegates to the storage's get_mut() method.
    /// The type must exist in the storage chain for this to compile.
    ///
    /// Note: This bypasses event processing, so prefer sending events for state changes.
    ///
    pub fn model_mut<T, Index>(&mut self) -> &mut T
    where
        Storage: Selector<T, Index>,
    {
        self.models.get_mut()
    }

    /// Get the number of pending events
    #[must_use]
    pub fn pending_events(&self) -> usize {
        // Count both queue and channel
        let channel_count = self.event_rx.len();
        self.event_queue.len() + channel_count
    }

    /// Check if there are any pending events
    #[must_use]
    pub fn has_pending_events(&self) -> bool {
        !self.event_queue.is_empty() || !self.event_rx.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{EmptyStorage, Storage, StorageBuilder};

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
        ctx: &mut EventContext<TestEvent, TestEffect, Storage<CounterModel, EmptyStorage>>,
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
        let storage = EmptyStorage::new().with_model(CounterModel { count: 0 });

        let (mut core, _) = Core::new(counter_update, storage);

        // Test event handling
        let _command = core.handle_event(TestEvent::Increment);

        // Verify model was updated (using new model() API)
        let model: &CounterModel = core.model();
        assert_eq!(model.count, 1);

        // Also verify with original storage API (both should work)
        let storage_model: &CounterModel = core.storage().get();
        assert_eq!(storage_model.count, 1);

        // Verify command was created
        // Just check it's not None - specific command type depends on implementation
    }

    #[test]
    fn test_core_event_processing() {
        let storage = EmptyStorage::new().with_model(CounterModel { count: 0 });

        let (mut core, sender) = Core::new(counter_update, storage);

        // Send events
        sender.send(TestEvent::Increment).unwrap();
        sender.send(TestEvent::Increment).unwrap();
        sender.send(TestEvent::Decrement).unwrap();

        // Process all events
        let (processed, commands) = core.process_events();

        assert!(processed);
        assert_eq!(commands.len(), 3);

        // Verify final model state
        let model: &CounterModel = core.storage().get();
        assert_eq!(model.count, 1); // +1 +1 -1 = 1
    }

    #[test]
    fn test_core_pending_events() {
        let storage = EmptyStorage::new().with_model(CounterModel { count: 0 });

        let (mut core, sender) = Core::new(counter_update, storage);

        assert_eq!(core.pending_events(), 0);
        assert!(!core.has_pending_events());

        sender.send(TestEvent::Increment).unwrap();
        assert!(core.pending_events() > 0);
        assert!(core.has_pending_events());

        core.process_events();
        assert_eq!(core.pending_events(), 0);
        assert!(!core.has_pending_events());
    }

    #[test]
    fn test_core_model_access() {
        let storage = EmptyStorage::new().with_model(CounterModel { count: 42 });
        let (mut core, _) = Core::new(counter_update, storage);

        // Test immutable model access
        let model: &CounterModel = core.model();
        assert_eq!(model.count, 42);

        // Test explicit type annotation (relies on type inference for Index)
        let model_explicit: &CounterModel = core.model();
        assert_eq!(model_explicit.count, 42);

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

        type TestStorage = Storage<ConfigModel, Storage<UserModel, Storage<CounterModel, EmptyStorage>>>;

        // Update function for the multi-model storage
        fn multi_model_update(
            event: TestEvent,
            ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
        ) -> Command<TestEvent, TestEffect> {
            // Just update the counter for simplicity
            let counter: &mut CounterModel = ctx.model_mut();
            match event {
                TestEvent::Increment => {
                    counter.count += 1;
                    Command::effect(TestEffect::Log)
                }
                TestEvent::Decrement => {
                    counter.count -= 1;
                    Command::effect(TestEffect::Log)
                }
            }
        }

        // Create storage with multiple models
        let storage = EmptyStorage::new()
            .with_model(CounterModel { count: 10 })
            .with_model(UserModel {
                name: "Alice".to_string(),
                age: 25,
            })
            .with_model(ConfigModel {
                theme: "dark".to_string(),
                debug: true,
            });

        let (mut core, _) = Core::new(multi_model_update, storage);

        // Test accessing different models
        let counter: &CounterModel = core.model();
        assert_eq!(counter.count, 10);

        let user: &UserModel = core.model();
        assert_eq!(user.name, "Alice");
        assert_eq!(user.age, 25);

        let config: &ConfigModel = core.model();
        assert_eq!(config.theme, "dark");
        assert!(config.debug);

        // Test mutable access to different models
        {
            let counter_mut: &mut CounterModel = core.model_mut();
            counter_mut.count = 999;

            let user_mut: &mut UserModel = core.model_mut();
            user_mut.name = "Bob".to_string();
            user_mut.age = 30;

            let config_mut: &mut ConfigModel = core.model_mut();
            config_mut.theme = "light".to_string();
            config_mut.debug = false;
        }

        // Verify all mutations worked
        let counter_after: &CounterModel = core.model();
        assert_eq!(counter_after.count, 999);

        let user_after: &UserModel = core.model();
        assert_eq!(user_after.name, "Bob");
        assert_eq!(user_after.age, 30);

        let config_after: &ConfigModel = core.model();
        assert_eq!(config_after.theme, "light");
        assert!(!config_after.debug);
    }

    #[test]
    fn test_core_model_vs_storage_api() {
        let storage = EmptyStorage::new().with_model(CounterModel { count: 0 });
        let (mut core, _) = Core::new(counter_update, storage);

        // Test both APIs work equivalently
        let model_via_storage: &CounterModel = core.storage().get();
        let model_via_model: &CounterModel = core.model();

        assert_eq!(model_via_storage.count, model_via_model.count);

        // Mutate via new API
        {
            let model_mut: &mut CounterModel = core.model_mut();
            model_mut.count = 123;
        }

        // Verify via both APIs
        let storage_model_after: &CounterModel = core.storage().get();
        let direct_model_after: &CounterModel = core.model();
        assert_eq!(storage_model_after.count, 123);
        assert_eq!(direct_model_after.count, 123);
    }
}
