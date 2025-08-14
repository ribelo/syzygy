use crossbeam_channel::Sender;
use crate::error::SyzygyError;

/// Handle for dispatching commands to a Syzygy instance
///
/// This handle can be cloned and shared across threads to send
/// commands to the Syzygy event processor.
#[derive(Debug)]
pub struct SyzygyHandle<C> {
    command_tx: Sender<C>,
}

impl<C> Clone for SyzygyHandle<C> {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
        }
    }
}

impl<C> SyzygyHandle<C> {
    pub(crate) fn new(command_tx: Sender<C>) -> Self {
        Self { command_tx }
    }

    /// Dispatch a command to be processed
    ///
    /// Commands are queued and will be processed when `process_commands`
    /// or similar methods are called on the Syzygy instance.
    ///
    /// Returns an error if the channel is closed (system is shutting down).
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use syzygy::prelude::*;
    /// # struct MyCommand;
    /// # let handle: SyzygyHandle<MyCommand> = todo!();
    /// handle.dispatch(MyCommand)?;
    /// ```
    pub fn dispatch(&self, command: C) -> Result<(), SyzygyError> {
        self.command_tx.send(command).map_err(|_| SyzygyError::ChannelClosed)
    }
    
    /// Try to dispatch a command without blocking
    ///
    /// Returns an error if the channel is full or closed.
    pub fn try_dispatch(&self, command: C) -> Result<(), SyzygyError> {
        self.command_tx.try_send(command).map_err(|e| e.into())
    }

    /// Get the command sender
    pub fn command_sender(&self) -> &Sender<C> {
        &self.command_tx
    }
}