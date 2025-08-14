//! Error types for Syzygy operations
//!
//! This module provides comprehensive error handling for different failure modes
//! that can occur during Syzygy operations.

use std::fmt;

/// Comprehensive error type for Syzygy operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyzygyError {
    /// Command channel is closed (system is shutting down)
    ChannelClosed,
    
    /// Command panicked during execution
    CommandPanic(String),
    
    /// Effect execution failed
    EffectFailed(String),
    
    /// Channel is full (for bounded channels)
    ChannelFull,
    
    /// Invalid state transition attempted
    InvalidStateTransition(String),
    
    /// Task tracker is closed (no new tasks can be spawned)
    TaskTrackerClosed,
    
    /// Resource was not found
    ResourceNotFound(String),
}

impl fmt::Display for SyzygyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyzygyError::ChannelClosed => {
                write!(f, "Channel is closed - system may be shutting down")
            }
            SyzygyError::CommandPanic(msg) => {
                write!(f, "Command panicked: {}", msg)
            }
            SyzygyError::EffectFailed(msg) => {
                write!(f, "Effect execution failed: {}", msg)
            }
            SyzygyError::ChannelFull => {
                write!(f, "Channel is full - system may be overloaded")
            }
            SyzygyError::InvalidStateTransition(msg) => {
                write!(f, "Invalid state transition: {}", msg)
            }
            SyzygyError::TaskTrackerClosed => {
                write!(f, "Task tracker is closed - no new tasks can be spawned")
            }
            SyzygyError::ResourceNotFound(msg) => {
                write!(f, "Resource not found: {}", msg)
            }
        }
    }
}

impl std::error::Error for SyzygyError {}

/// Legacy alias for channel closed error
/// 
/// This maintains compatibility while providing a path to the more comprehensive
/// error system.
pub type ChannelClosed = SyzygyError;

/// Specific error type for resource operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceNotFoundError {
    pub type_name: String,
}

impl ResourceNotFoundError {
    pub fn new<T>() -> Self {
        Self {
            type_name: std::any::type_name::<T>().to_string(),
        }
    }
}

impl fmt::Display for ResourceNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Resource of type '{}' was not found", self.type_name)
    }
}

impl std::error::Error for ResourceNotFoundError {}

impl From<ResourceNotFoundError> for SyzygyError {
    fn from(err: ResourceNotFoundError) -> Self {
        SyzygyError::ResourceNotFound(err.type_name)
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for SyzygyError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        SyzygyError::ChannelClosed
    }
}

impl<T> From<crossbeam_channel::TrySendError<T>> for SyzygyError {
    fn from(err: crossbeam_channel::TrySendError<T>) -> Self {
        match err {
            crossbeam_channel::TrySendError::Full(_) => SyzygyError::ChannelFull,
            crossbeam_channel::TrySendError::Disconnected(_) => SyzygyError::ChannelClosed,
        }
    }
}