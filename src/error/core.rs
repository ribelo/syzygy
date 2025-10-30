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

impl<T> From<std::sync::mpsc::SendError<T>> for CoreError {
    fn from(_: std::sync::mpsc::SendError<T>) -> Self {
        CoreError::ChannelClosed
    }
}
