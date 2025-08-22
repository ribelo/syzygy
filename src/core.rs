use std::collections::VecDeque;
use crossbeam_channel::{Sender, Receiver, unbounded};
use crate::app::App;
use crate::command::Command;

#[cfg(feature = "tracing")]
use tracing::{debug, span, Level};

/// Core handles synchronous event processing and owns the application model.
/// 
/// Core is designed to be used on any thread, including UI threads, as it
/// never blocks and all operations are synchronous. It processes events,
/// updates the model, and returns Commands describing effects to execute.
pub struct Core<A: App> {
    /// The application instance
    app: A,
    
    /// The application model - owned and mutable
    model: A::Model,
    
    /// Queue of events to process
    event_queue: VecDeque<A::Event>,
    
    /// Pre-allocated command buffer to avoid reallocations
    command_buffer: Vec<Command<A::Event, A::Effect>>,
    
    /// Channel for receiving external events
    event_rx: Receiver<A::Event>,
    
    /// Channel for sending events (kept for cloning)
    event_tx: Sender<A::Event>,
}

impl<A: App> Core<A> {
    /// Create a new Core with specific app and model
    /// 
    /// Note: This is the primary constructor. Users must provide both app and model instances.
    pub fn new(app: A, model: A::Model) -> (Self, Sender<A::Event>) {
        let (event_tx, event_rx) = unbounded();
        
        let core = Self {
            app,
            model,
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
    pub fn handle_event(&mut self, event: A::Event) -> Command<A::Event, A::Effect> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "handle_event").entered();
        
        #[cfg(feature = "tracing")]
        debug!("Processing event");
        
        let command = self.app.update(event, &mut self.model);
        
        #[cfg(feature = "tracing")]
        debug!("Event processed, command created");
        
        command
    }
    
    /// Get the current view model
    /// 
    /// This transforms the current model into a view representation for UI rendering.
    /// Only available when the "view-model" feature is enabled.
    #[cfg(feature = "view-model")]
    pub fn view(&self) -> A::ViewModel {
        self.app.view(&self.model)
    }
    
    /// Get a reference to the current model
    pub fn model(&self) -> &A::Model {
        &self.model
    }
    
    /// Get a mutable reference to the current model
    /// 
    /// This should be used carefully as it bypasses the event system.
    pub fn model_mut(&mut self) -> &mut A::Model {
        &mut self.model
    }
    
    /// Queue an event for later processing
    pub fn queue_event(&mut self, event: A::Event) {
        self.event_queue.push_back(event);
    }
    
    /// Process all queued events
    /// 
    /// Returns Commands from processing all queued events.
    /// Uses pre-allocated buffer to reduce reallocations.
    pub fn process_queued_events(&mut self) -> Vec<Command<A::Event, A::Effect>> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "process_queued_events").entered();
        
        // Clear buffer but keep capacity - O(1) operation
        self.command_buffer.clear();
        let _event_count = self.event_queue.len();
        
        #[cfg(feature = "tracing")]
        debug!(_event_count, "Processing queued events");
        
        while let Some(event) = self.event_queue.pop_front() {
            let command = self.handle_event(event);
            self.command_buffer.push(command);
        }
        
        #[cfg(feature = "tracing")]
        debug!(command_count = self.command_buffer.len(), "Finished processing events");
        
        // Take ownership of commands from buffer (avoids cloning Commands which can't be cloned)
        std::mem::take(&mut self.command_buffer)
    }
    
    /// Check for external events and add them to the queue
    /// 
    /// This is non-blocking and will return immediately if no events are available.
    pub fn poll_external_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_queue.push_back(event);
        }
    }
    
    /// Process one external event if available
    /// 
    /// This is useful for integrating with UI event loops.
    pub fn process_one_external_event(&mut self) -> Option<Command<A::Event, A::Effect>> {
        if let Ok(event) = self.event_rx.try_recv() {
            Some(self.handle_event(event))
        } else {
            None
        }
    }
    
    /// Get a sender for external events
    pub fn event_sender(&self) -> Sender<A::Event> {
        self.event_tx.clone()
    }
    
    /// Send an event directly (convenience method)
    pub fn send_event(&self, event: A::Event) -> Result<(), crate::error::CoreError> {
        self.event_tx.send(event)
            .map_err(|_| crate::error::CoreError::ChannelClosed)
    }
    
    /// Check if the event queue is empty
    /// 
    /// Useful for optimizing event loop polling.
    pub fn has_queued_events(&self) -> bool {
        !self.event_queue.is_empty()
    }
}