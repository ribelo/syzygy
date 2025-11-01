use thiserror::Error;

/// Errors that can occur in Core operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    /// Event channel is closed (system is shutting down)
    #[error("Event channel is closed - system may be shutting down")]
    ChannelClosed,

    /// Event channel is full (for bounded channels)
    #[error("Event channel is full - system may be overloaded")]
    ChannelFull,
}

impl<T> From<crossbeam_channel::SendError<T>> for CoreError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        CoreError::ChannelClosed
    }
}

impl<T> From<crossbeam_channel::TrySendError<T>> for CoreError {
    fn from(error: crossbeam_channel::TrySendError<T>) -> Self {
        match error {
            crossbeam_channel::TrySendError::Full(_) => CoreError::ChannelFull,
            crossbeam_channel::TrySendError::Disconnected(_) => CoreError::ChannelClosed,
        }
    }
}
