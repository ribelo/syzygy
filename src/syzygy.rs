//! THE Syzygy system - simple single model implementation
//!
//! This module provides the primary Syzygy implementation with a single model
//! for clean, maintainable event handling.

use crate::dispatch::Dispatch;
use crate::context::CommandContext;
use crate::resource::Resources;
use crossbeam_channel::{Receiver, Sender};
use std::future::Future;
use std::pin::Pin;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[cfg(feature = "tracing")]
use tracing::{debug, trace, warn};

// ============================================================================
// THE Syzygy Implementation - Single Model
// ============================================================================

/// THE unified Syzygy system
///
/// Generic over:
/// - M: Model (application state)
/// - E: Event enum (all possible events)
/// - C: Command enum (all possible side effects)
/// - R: Resources (shared state and services)
pub struct Syzygy<M, E, C, R = Resources>
where
    E: Clone,
{
    pub(crate) model: M,
    pub(crate) resources: R,
    pub(crate) event_rx: Receiver<E>,
    pub(crate) event_tx: Sender<E>,
    pub(crate) command_tx: Sender<C>,
    pub(crate) event_handler: fn(E, &mut M) -> Dispatch<E, C>,
    pub(crate) command_handler: Option<fn(CommandContext<E, R>, C) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    // Monitoring fields
    pub(crate) processed_events_count: u64,
}

impl<M, E, C, R> std::fmt::Debug for Syzygy<M, E, C, R>
where
    E: Clone,
    M: std::fmt::Debug,
    R: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syzygy")
            .field("model", &self.model)
            .field("resources", &self.resources)
            .field("event_rx", &"<channel>")
            .field("event_tx", &"<channel>")
            .field("command_tx", &"<channel>")
            .field("event_handler", &format!("<fn@{:p}>", self.event_handler as *const ()))
            .field("command_handler", &if self.command_handler.is_some() {
                format!("<fn@{:p}>", self.command_handler.unwrap() as *const ())
            } else {
                "None".to_string()
            })
            .field("processed_events_count", &self.processed_events_count)
            .finish()
    }
}

impl<E, C> Syzygy<(), E, C, Resources>
where
    E: Send + Clone + 'static,
    C: Send + Clone + 'static,
{
    /// Create a new builder for Syzygy
    pub fn builder() -> crate::builder::SyzygyBuilder<(), E, C, Resources, crate::builder::InitialStage> {
        crate::builder::SyzygyBuilder::new()
    }
}

impl<M, E, C, R> Syzygy<M, E, C, R>
where
    E: Clone,
{
    /// Get a reference to the model
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Get a mutable reference to the model
    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    /// Get a reference to the resources
    pub fn resources(&self) -> &R {
        &self.resources
    }

    /// Check if there are pending events
    pub fn has_pending_events(&self) -> bool {
        !self.event_rx.is_empty()
    }

    /// Get the number of pending events
    pub fn pending_event_count(&self) -> usize {
        self.event_rx.len()
    }
    
    /// Get the total number of processed events
    pub fn total_events_processed(&self) -> u64 {
        self.processed_events_count
    }
    
    /// Check if the system is idle (no pending events)
    pub fn is_idle(&self) -> bool {
        self.event_rx.is_empty()
    }
    
    /// Get the queue depth (same as pending_event_count, for monitoring)
    pub fn queue_depth(&self) -> usize {
        self.pending_event_count()
    }
    
    /// Get the number of pending commands (if available)
    pub fn pending_commands_count(&self) -> usize {
        // Commands are sent to executor immediately, so this reflects channel buffer
        // This is an approximation as we can't directly inspect command channel
        0 // TODO: Could be enhanced with command tracking if needed
    }
}

impl<M, E, C, R> Syzygy<M, E, C, R>
where
    E: Send + Clone,
    C: Send + Clone,
{
    /// Process all pending events
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub fn process_events(&mut self) {
        let mut _processed_count = 0;

        while let Ok(event) = self.event_rx.try_recv() {
            _processed_count += 1;

            // Process single event with typed handler
            let result = self.process_single_event(event);

            #[cfg(feature = "tracing")]
            trace!(
                events_count = result.events.len(),
                commands_count = result.commands.len(),
                "Event processed successfully"
            );

            // Queue new events back for processing
            for new_event in result.events {
                if let Err(_) = self.event_tx.send(new_event) {
                    #[cfg(feature = "tracing")]
                    warn!("Failed to queue new event - channel closed");
                    break; // Stop processing if event channel is closed
                }
            }

            // Send commands to executor
            for command in result.commands {
                if let Err(_) = self.command_tx.send(command) {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Failed to send command - channel closed");
                    break; // Stop processing if command channel is closed
                }
            }
        }

        #[cfg(feature = "tracing")]
        if _processed_count > 0 {
            debug!(
                processed_events = _processed_count,
                "Finished processing events"
            );
        }
    }

    /// Process a single event with the stored event handler
    ///
    /// This method processes the model state using the stored handler.
    /// If the handler panics, the panic is caught and logged, and an empty Dispatch is returned.
    pub fn process_single_event(&mut self, event: E) -> Dispatch<E, C>
    where
        E: Clone,
    {
        // Increment processed count
        self.processed_events_count += 1;
        
        // Call the handler with panic recovery
        let result = catch_unwind(AssertUnwindSafe(|| {
            (self.event_handler)(event, &mut self.model)
        }));
        
        match result {
            Ok(dispatch) => dispatch,
            Err(_panic_payload) => {
                #[cfg(feature = "tracing")]
                tracing::error!("Event handler panicked, continuing with empty dispatch");
                
                // Log to standard error as well for visibility
                eprintln!("SYZYGY PANIC: Event handler panicked but system continues running");
                
                // Return empty dispatch to continue processing
                Dispatch::none()
            }
        }
    }
}
