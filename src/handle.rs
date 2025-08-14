use crossbeam_channel::Sender;
use crate::error::SyzygyError;

/// Handle for dispatching events to a Syzygy instance
///
/// This handle can be cloned and shared across threads to send
/// events to the Syzygy event processor.
#[derive(Debug)]
pub struct SyzygyHandle<E> {
    event_tx: Sender<E>,
}

impl<E> Clone for SyzygyHandle<E> {
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
        }
    }
}

impl<E> SyzygyHandle<E> {
    pub(crate) fn new(event_tx: Sender<E>) -> Self {
        Self { event_tx }
    }

    /// Dispatch an event to be processed
    ///
    /// Events are queued and will be processed when `process_events`
    /// or similar methods are called on the Syzygy instance.
    ///
    /// Returns an error if the channel is closed (system is shutting down).
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use syzygy::prelude::*;
    /// # struct MyEvent;
    /// # let handle: SyzygyHandle<MyEvent> = todo!();
    /// handle.dispatch(MyEvent)?;
    /// ```
    pub fn dispatch(&self, event: E) -> Result<(), SyzygyError> {
        self.event_tx.send(event).map_err(|_| SyzygyError::ChannelClosed)
    }
    
    /// Try to dispatch an event without blocking
    ///
    /// Returns an error if the channel is full or closed.
    pub fn try_dispatch(&self, event: E) -> Result<(), SyzygyError> {
        self.event_tx.try_send(event).map_err(|e| e.into())
    }

    /// Get the event sender
    pub fn event_sender(&self) -> &Sender<E> {
        &self.event_tx
    }
}