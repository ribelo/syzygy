use std::fmt;

/// Errors that can occur during Command execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Command panicked during execution
    CommandPanic(String),
    
    /// Command execution was cancelled
    Cancelled,
    
    /// Command timed out
    Timeout,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::CommandPanic(msg) => {
                write!(f, "Command panicked: {msg}")
            }
            CommandError::Cancelled => {
                write!(f, "Command execution was cancelled")
            }
            CommandError::Timeout => {
                write!(f, "Command execution timed out")
            }
        }
    }
}

impl std::error::Error for CommandError {}