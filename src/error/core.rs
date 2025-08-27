use std::fmt;

/// Errors that can occur in Core operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// Event channel is closed (system is shutting down)
    ChannelClosed,

    /// Event channel is full (for bounded channels)
    ChannelFull,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::ChannelClosed => {
                write!(f, "Event channel is closed - system may be shutting down")
            }
            CoreError::ChannelFull => {
                write!(f, "Event channel is full - system may be overloaded")
            }
        }
    }
}

impl std::error::Error for CoreError {}

impl<T> From<crossbeam_channel::SendError<T>> for CoreError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        CoreError::ChannelClosed
    }
}

impl<T> From<crossbeam_channel::TrySendError<T>> for CoreError {
    fn from(err: crossbeam_channel::TrySendError<T>) -> Self {
        match err {
            crossbeam_channel::TrySendError::Full(_) => CoreError::ChannelFull,
            crossbeam_channel::TrySendError::Disconnected(_) => CoreError::ChannelClosed,
        }
    }
}
