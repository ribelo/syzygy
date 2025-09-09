use thiserror::Error;

/// Errors that can occur in Shell operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShellError {
    /// Task tracker is closed (no new tasks can be spawned)
    #[error("Task tracker is closed - no new tasks can be spawned")]
    TaskTrackerClosed,

    /// Task tracker mutex was poisoned
    #[error("Task tracker mutex was poisoned")]
    TaskTrackerPoisoned,

    /// Event channel is closed
    #[error("Event channel is closed")]
    EventChannelClosed,

    /// Effect execution failed
    #[error("Effect execution failed: {0}")]
    EffectFailed(String),

    /// Invalid state transition attempted
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),

    /// Task spawn failed
    #[error("Task spawn failed: {0}")]
    TaskSpawnFailed(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    CommandExecutionFailed(String),
}
