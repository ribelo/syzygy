use std::time::Duration;

use thiserror::Error;

/// Errors that can occur in Shell operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShellError {
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

    /// Abortable blocking work must use cooperative cancellation
    #[error("abortable effect resolved to Task::blocking; use Task::blocking_cooperative for lease-owned blocking work")]
    AbortableBlockingTask,

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    CommandExecutionFailed(String),

    /// Process control step addressed a missing or non-interactive process
    #[error("Invalid process control command: {0}")]
    InvalidProcessControl(String),

    /// Subscription handler returned the same key more than once
    #[error("Duplicate subscription key in desired state: {0}")]
    DuplicateSubscriptionKey(String),

    /// Subscription driver type was referenced but never registered
    #[error("Missing subscription driver: {0}")]
    MissingSubscriptionDriver(String),

    /// Effect queue reached its configured capacity
    #[error(
        "Effect queue is full (capacity {capacity}). Consider increasing the capacity or awaiting idle before queuing more effects"
    )]
    EffectQueueFull { capacity: usize },

    /// Timed out while waiting for work to complete
    #[error("Timed out after {duration:?} while draining work; use larger timeouts or inspect backpressure metrics")]
    Timeout { duration: Duration },
}
