use thiserror::Error;

/// Errors that can occur in Core operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    #[error("Event channel is closed - system may be shutting down")]
    ChannelClosed,

    #[error("Event channel is full - system may be overloaded")]
    ChannelFull,
}
