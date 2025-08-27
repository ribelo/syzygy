use std::fmt;

/// Errors that can occur in Shell operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellError {
    /// Task tracker is closed (no new tasks can be spawned)
    TaskTrackerClosed,

    /// Task tracker mutex was poisoned
    TaskTrackerPoisoned,

    /// Event channel is closed
    EventChannelClosed,

    /// Effect execution failed
    EffectFailed(String),

    /// Invalid state transition attempted
    InvalidStateTransition(String),

    /// Task spawn failed
    TaskSpawnFailed(String),

    /// Command execution failed
    CommandExecutionFailed(String),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellError::TaskTrackerClosed => {
                write!(f, "Task tracker is closed - no new tasks can be spawned")
            }
            ShellError::TaskTrackerPoisoned => {
                write!(f, "Task tracker mutex was poisoned")
            }
            ShellError::EventChannelClosed => {
                write!(f, "Event channel is closed")
            }
            ShellError::EffectFailed(msg) => {
                write!(f, "Effect execution failed: {msg}")
            }
            ShellError::InvalidStateTransition(msg) => {
                write!(f, "Invalid state transition: {msg}")
            }
            ShellError::TaskSpawnFailed(msg) => {
                write!(f, "Task spawn failed: {msg}")
            }
            ShellError::CommandExecutionFailed(msg) => {
                write!(f, "Command execution failed: {msg}")
            }
        }
    }
}

impl std::error::Error for ShellError {}
