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
    use crate::storage_type;

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

    // ============================================================================
    // storage_type! Macro Tests
    // ============================================================================

    #[derive(Debug, Clone, PartialEq)]
    struct TestModel1(i32);

    #[derive(Debug, Clone, PartialEq)]
    struct TestModel2(String);

    #[derive(Debug, Clone, PartialEq)]
    struct TestModel3(bool);

    #[derive(Debug, Clone, PartialEq)]
    struct TestModel4(f64);

    #[test]
    fn test_storage_type_single() {
        type SingleStorage = storage_type!(TestModel1);
        
        let storage = EmptyStorage::new()
            .with_model(TestModel1(42));
        
        let _: SingleStorage = storage;
        
        let model: &TestModel1 = storage.get();
        assert_eq!(model.0, 42);
    }

    #[test]
    fn test_storage_type_double() {
        type DoubleStorage = storage_type!(TestModel1, TestModel2);
        
        let storage = EmptyStorage::new()
            .with_model(TestModel1(42))
            .with_model(TestModel2("hello".to_string()));
        
        let _: DoubleStorage = storage;
        
        let model1: &TestModel1 = storage.get();
        let model2: &TestModel2 = storage.get();
        
        assert_eq!(model1.0, 42);
        assert_eq!(model2.0, "hello");
    }

    #[test]
    fn test_storage_type_triple() {
        type TripleStorage = storage_type!(TestModel1, TestModel2, TestModel3);
        
        let storage = EmptyStorage::new()
            .with_model(TestModel1(42))
            .with_model(TestModel2("hello".to_string()))
            .with_model(TestModel3(true));
        
        let _: TripleStorage = storage;
        
        let model1: &TestModel1 = storage.get();
        let model2: &TestModel2 = storage.get();
        let model3: &TestModel3 = storage.get();
        
        assert_eq!(model1.0, 42);
        assert_eq!(model2.0, "hello");
        assert_eq!(model3.0, true);
    }

    #[test]
    fn test_storage_type_quad() {
        type QuadStorage = storage_type!(TestModel1, TestModel2, TestModel3, TestModel4);
        
        let storage = EmptyStorage::new()
            .with_model(TestModel1(42))
            .with_model(TestModel2("hello".to_string()))
            .with_model(TestModel3(true))
            .with_model(TestModel4(3.14));
        
        let _: QuadStorage = storage;
        
        let model1: &TestModel1 = storage.get();
        let model2: &TestModel2 = storage.get();
        let model3: &TestModel3 = storage.get();
        let model4: &TestModel4 = storage.get();
        
        assert_eq!(model1.0, 42);
        assert_eq!(model2.0, "hello");
        assert_eq!(model3.0, true);
        assert_eq!(model4.0, 3.14);
    }

    #[test]
    fn test_storage_type_equivalent_to_manual() {
        type ManualStorage = Storage<TestModel3, Storage<TestModel2, Storage<TestModel1, EmptyStorage>>>;
        type MacroStorage = storage_type!(TestModel1, TestModel2, TestModel3);
        
        // Both types should be equivalent
        let manual_storage: ManualStorage = EmptyStorage::new()
            .with_model(TestModel1(1))
            .with_model(TestModel2("test".to_string()))
            .with_model(TestModel3(false));
        
        let macro_storage: MacroStorage = EmptyStorage::new()
            .with_model(TestModel1(1))
            .with_model(TestModel2("test".to_string()))
            .with_model(TestModel3(false));
        
        // Verify they work the same way
        let manual_m1: &TestModel1 = manual_storage.get();
        let macro_m1: &TestModel1 = macro_storage.get();
        assert_eq!(manual_m1, macro_m1);
        
        let manual_m2: &TestModel2 = manual_storage.get();
        let macro_m2: &TestModel2 = macro_storage.get();
        assert_eq!(manual_m2, macro_m2);
        
        let manual_m3: &TestModel3 = manual_storage.get();
        let macro_m3: &TestModel3 = macro_storage.get();
        assert_eq!(manual_m3, macro_m3);
    }

    #[test]
    fn test_storage_type_contains() {
        type TestStorage = storage_type!(TestModel1, TestModel2, TestModel3);
        
        let storage = EmptyStorage::new()
            .with_model(TestModel1(42))
            .with_model(TestModel2("hello".to_string()))
            .with_model(TestModel3(true));
        
        let _: TestStorage = storage;
        
        assert!(storage.contains::<TestModel1>());
        assert!(storage.contains::<TestModel2>());
        assert!(storage.contains::<TestModel3>());
        assert!(!storage.contains::<TestModel4>());
    }

    #[test]
    fn test_storage_type_mutable_access() {
        type TestStorage = storage_type!(TestModel1, TestModel2);
        
        let storage = EmptyStorage::new()
            .with_model(TestModel1(1))
            .with_model(TestModel2("initial".to_string()));
        
        let _: TestStorage = storage;
        
        {
            let model1: &mut TestModel1 = storage.get_mut();
            model1.0 = 100;
            
            let model2: &mut TestModel2 = storage.get_mut();
            model2.0 = "modified".to_string();
        }
        
        let model1: &TestModel1 = storage.get();
        let model2: &TestModel2 = storage.get();
        
        assert_eq!(model1.0, 100);
        assert_eq!(model2.0, "modified");
    }

    #[test]
    #[should_panic(expected = "Duplicate type in chain")]
    fn test_storage_type_duplicate_detection() {
        type TestStorage = storage_type!(TestModel1, TestModel2);
        
        let _storage = EmptyStorage::new()
            .with_model(TestModel1(1))
            .with_model(TestModel2("test".to_string()))
            .with_model(TestModel1(2)); // Should panic - duplicate TestModel1
    }
}