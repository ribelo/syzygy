use thiserror::Error;

/// Errors that can occur during Command execution
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    /// Command panicked during execution
    #[error("Command panicked: {0}")]
    CommandPanic(String),

    /// Command timed out
    #[error("Command execution timed out")]
    Timeout,
}
