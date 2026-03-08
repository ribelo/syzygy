use std::time::Duration;

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

    /// Runtime initialization failed
    #[error("Runtime initialization failed: {0}")]
    RuntimeInitializationFailed(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    CommandExecutionFailed(String),

    /// Effect queue reached its configured capacity
    #[error(
        "Effect queue is full (capacity {capacity}). Consider increasing the capacity or awaiting idle before queuing more effects"
    )]
    EffectQueueFull { capacity: usize },

    /// Timed out while waiting for work to complete
    #[error("Timed out after {duration:?} while draining work; use larger timeouts or inspect backpressure metrics")]
    Timeout { duration: Duration },
}
