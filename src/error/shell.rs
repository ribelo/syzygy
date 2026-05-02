use std::time::Duration;

use thiserror::Error;

/// Errors that can occur in Shell operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShellError {
    /// Event channel is closed
    #[error("Event channel is closed")]
    EventChannelClosed,

    /// Runtime initialization failed
    #[error("Runtime initialization failed: {0}")]
    RuntimeInitializationFailed(String),

    /// Effect was dispatched without any handler claiming it
    #[error("Unhandled effect reached the shell: {effect_type}")]
    UnhandledEffect { effect_type: &'static str },

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

    /// Timed out while waiting for work to complete
    #[error("Timed out after {duration:?} while draining work; use larger timeouts or inspect backpressure metrics")]
    Timeout { duration: Duration },

    /// Deferred event queue exceeded its bounded backlog.
    #[error(
        "Deferred event backlog overflowed at limit {limit}; dropped_events={dropped_events}. Adjust event flow or configure explicit overflow policy"
    )]
    DeferredEventOverflow { limit: usize, dropped_events: usize },
}
