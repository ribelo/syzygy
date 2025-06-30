//! Function-specific error types for Syzygy operations
//!
//! Each function defines its own error type that represents only the failures
//! that specific function can produce. This follows the principle that each
//! function should own its error handling completely.

use std::any::TypeId;

/// Error for resource lookup operations
#[derive(Debug, thiserror::Error)]
#[error("Resource of type {type_name} not found")]
pub struct ResourceNotFoundError {
    pub type_name: &'static str,
    pub type_id: TypeId,
}

impl ResourceNotFoundError {
    pub fn new<T: 'static>() -> Self {
        Self {
            type_name: std::any::type_name::<T>(),
            type_id: TypeId::of::<T>(),
        }
    }
}

/// Error for effect dispatch operations
#[derive(Debug, thiserror::Error)]
#[error("Failed to dispatch effect: {reason}")]
pub struct DispatchError {
    pub reason: String,
}

impl DispatchError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// Error for context creation operations  
#[derive(Debug, thiserror::Error)]
#[error("Failed to create context: {reason}")]
pub struct ContextCreationError {
    pub reason: String,
}

impl ContextCreationError {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// Error for lock acquisition operations
#[derive(Debug, thiserror::Error)]
#[error("Failed to acquire {lock_type} lock: {reason}")]
pub struct LockAcquisitionError {
    pub lock_type: String,
    pub reason: String,
}

impl LockAcquisitionError {
    pub fn new(lock_type: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            lock_type: lock_type.into(),
            reason: reason.into(),
        }
    }
}

/// Error for resource replacement operations
#[derive(Debug, thiserror::Error)]
#[error("Failed to replace resource of type {type_name}: {reason}")]
pub struct ResourceReplaceError {
    pub type_name: &'static str,
    pub reason: String,
}

impl ResourceReplaceError {
    pub fn new<T: 'static>(reason: impl Into<String>) -> Self {
        Self {
            type_name: std::any::type_name::<T>(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_not_found_error() {
        let err = ResourceNotFoundError::new::<String>();
        assert_eq!(err.type_name, std::any::type_name::<String>());
        assert_eq!(err.type_id, TypeId::of::<String>());
        assert!(err.to_string().contains("alloc::string::String"));
    }

    #[test]
    fn test_dispatch_error() {
        let err = DispatchError::new("channel closed");
        assert_eq!(err.reason, "channel closed");
        assert!(err.to_string().contains("channel closed"));
    }

    #[test]
    fn test_context_creation_error() {
        let err = ContextCreationError::new("model snapshot failed");
        assert!(err.to_string().contains("model snapshot failed"));
    }

    #[test]
    fn test_lock_acquisition_error() {
        let err = LockAcquisitionError::new("read", "poisoned");
        assert!(err.to_string().contains("read"));
        assert!(err.to_string().contains("poisoned"));
    }

    #[test]
    fn test_resource_replace_error() {
        let err = ResourceReplaceError::new::<i32>("type mismatch");
        assert_eq!(err.type_name, std::any::type_name::<i32>());
        assert!(err.to_string().contains("type mismatch"));
    }
}