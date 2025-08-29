//! Model and Resource Storage using the generic chain implementation
//!
//! This provides type-safe storage for models and resources using the
//! unified generic chain implementation.

use super::generic::{
    Chain, EmptyChain, ChainBuilder
};

// ============================================================================
// Type Aliases and Constants
// ============================================================================

/// Terminal type for model/resource chains
pub type EmptyStorage = EmptyChain;

/// Generic storage chain for models and resources
pub type Storage<Head, Tail> = Chain<Head, Tail>;


/// Builder trait for storage chains
pub trait StorageBuilder<T> {
    /// The type after adding a model
    type Output;

    /// Add a model with runtime duplicate checking
    fn with_model(self, model: T) -> Self::Output;
}

// Implement StorageBuilder in terms of ChainBuilder
impl<T, S> StorageBuilder<T> for S
where
    S: ChainBuilder<T>,
{
    type Output = <S as ChainBuilder<T>>::Output;

    fn with_model(self, model: T) -> Self::Output {
        self.with_item(model)
    }
}

// Re-export index types and traits for convenience  
pub use super::generic::{Here, There};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::generic::RuntimeContains;

    #[derive(Debug, Clone, PartialEq)]
    struct Model1 {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Model2 {
        name: String,
    }

    #[test]
    fn test_empty_storage() {
        let storage = EmptyStorage::new();
        assert!(!storage.contains::<Model1>());
    }

    #[test]
    fn test_single_model() {
        let storage = EmptyStorage::new().with_model(Model1 { value: 42 });
        
        let model: &Model1 = storage.get();
        assert_eq!(model.value, 42);
        
        assert!(storage.contains::<Model1>());
        assert!(!storage.contains::<Model2>());
    }

    #[test]
    fn test_multiple_models() {
        let storage = EmptyStorage::new()
            .with_model(Model1 { value: 1 })
            .with_model(Model2 { name: "test".to_string() });
        
        let model1: &Model1 = storage.get();
        let model2: &Model2 = storage.get();
        
        assert_eq!(model1.value, 1);
        assert_eq!(model2.name, "test");
        
        assert!(storage.contains::<Model1>());
        assert!(storage.contains::<Model2>());
    }

    #[test]
    fn test_mutable_access() {
        let storage = EmptyStorage::new()
            .with_model(Model1 { value: 1 });
        
        {
            let model: &mut Model1 = storage.get_mut();
            model.value = 42;
        }
        
        let model: &Model1 = storage.get();
        assert_eq!(model.value, 42);
    }

    #[test]
    #[should_panic(expected = "Duplicate type in chain")]
    fn test_duplicate_detection() {
        let _storage = EmptyStorage::new()
            .with_model(Model1 { value: 1 })
            .with_model(Model1 { value: 2 }); // Should panic
    }

    #[test]
    fn test_clone() {
        let storage = EmptyStorage::new()
            .with_model(Model1 { value: 42 })
            .with_model(Model2 { name: "test".to_string() });
        
        let cloned = storage.clone();
        
        let model1: &Model1 = cloned.get();
        let model2: &Model2 = cloned.get();
        
        assert_eq!(model1.value, 42);
        assert_eq!(model2.name, "test");
    }
}