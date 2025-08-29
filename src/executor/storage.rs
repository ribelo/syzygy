//! Executor storage using the generic chain implementation
//!
//! This allows multiple executors to be stored type-safely and accessed by type
//! using the same generic chain implementation as model/resource storage.

use crate::storage::generic::{
    Chain, EmptyChain, ChainBuilder
};

// ============================================================================
// Type Aliases for Executor Storage
// ============================================================================

/// Terminal type for the executor chain
pub type EmptyExecutorStorage = EmptyChain;

/// Generic executor storage chain
pub type ExecutorStorage<Head, Tail> = Chain<Head, Tail>;

/// Builder trait for executor chains
pub trait StorageBuilder<T> {
    /// The type after adding an executor
    type Output;

    /// Add an executor with runtime duplicate checking
    fn with_executor(self, executor: T) -> Self::Output;
}

// Implement StorageBuilder in terms of ChainBuilder
impl<T, S> StorageBuilder<T> for S
where
    S: ChainBuilder<T>,
{
    type Output = <S as ChainBuilder<T>>::Output;

    fn with_executor(self, executor: T) -> Self::Output {
        self.with_item(executor)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::generic::RuntimeContains;

    #[derive(Debug, Clone)]
    struct MockExecutor1 {
        name: String,
    }

    #[derive(Debug, Clone)]
    struct MockExecutor2 {
        value: i32,
    }

    #[test]
    fn test_empty_executor_storage() {
        let storage = EmptyExecutorStorage::new();
        assert!(!storage.contains::<MockExecutor1>());
    }

    #[test]
    fn test_single_executor() {
        let storage = EmptyExecutorStorage::default().with_executor(MockExecutor1 {
            name: "test".to_string(),
        });
        
        let executor: &MockExecutor1 = storage.get();
        assert_eq!(executor.name, "test");
        
        assert!(storage.contains::<MockExecutor1>());
        assert!(!storage.contains::<MockExecutor2>());
    }

    #[test]
    fn test_multiple_executors() {
        let storage = EmptyExecutorStorage::default()
            .with_executor(MockExecutor1 { name: "test".to_string() })
            .with_executor(MockExecutor2 { value: 42 });
        
        let exec1: &MockExecutor1 = storage.get();
        let exec2: &MockExecutor2 = storage.get();
        
        assert_eq!(exec1.name, "test");
        assert_eq!(exec2.value, 42);
        
        assert!(storage.contains::<MockExecutor1>());
        assert!(storage.contains::<MockExecutor2>());
    }

    #[test]
    fn test_mutable_access() {
        let storage = EmptyExecutorStorage::default()
            .with_executor(MockExecutor2 { value: 1 });
        
        {
            let executor: &mut MockExecutor2 = storage.get_mut();
            executor.value = 42;
        }
        
        let executor: &MockExecutor2 = storage.get();
        assert_eq!(executor.value, 42);
    }

    #[test]
    #[should_panic(expected = "Duplicate type in chain")]
    fn test_duplicate_detection() {
        let _storage = EmptyExecutorStorage::default()
            .with_executor(MockExecutor1 { name: "first".to_string() })
            .with_executor(MockExecutor1 { name: "second".to_string() }); // Should panic
    }

    #[test]
    fn test_clone() {
        let storage = EmptyExecutorStorage::default()
            .with_executor(MockExecutor1 { name: "test".to_string() })
            .with_executor(MockExecutor2 { value: 42 });
        
        let cloned = storage.clone();
        
        let exec1: &MockExecutor1 = cloned.get();
        let exec2: &MockExecutor2 = cloned.get();
        
        assert_eq!(exec1.name, "test");
        assert_eq!(exec2.value, 42);
    }
}