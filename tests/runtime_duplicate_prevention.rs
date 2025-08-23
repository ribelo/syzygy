//! Tests for runtime duplicate prevention in ModelChain

use syzygy::storage::*;

// Test models
#[derive(Debug, Clone)]
pub struct TestModel1 {
    pub value: u64,
}

#[derive(Debug, Clone)]
pub struct TestModel2 {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TestModel3 {
    pub count: i32,
}

#[test]
fn test_runtime_contains_checking() {
    // Test empty chain
    let empty = EmptyStorage::default();
    assert!(!empty.contains::<TestModel1>());
    assert!(!empty.contains::<TestModel2>());
    
    // Test single element chain
    let single = EmptyStorage::default()
        .with_model_unchecked(TestModel1 { value: 1 });
    assert!(single.contains::<TestModel1>());
    assert!(!single.contains::<TestModel2>());
    
    // Test multi-element chain
    let multi = single
        .with_model_unchecked(TestModel2 { name: "test".to_string() })
        .with_model_unchecked(TestModel3 { count: 10 });
    assert!(multi.contains::<TestModel1>());
    assert!(multi.contains::<TestModel2>());
    assert!(multi.contains::<TestModel3>());
}

#[test]
fn test_try_with_model_success() {
    let chain = EmptyStorage::default()
        .with_model_unchecked(TestModel1 { value: 1 });
    
    // Adding different type should succeed
    let result = chain.try_with_model(TestModel2 { name: "test".to_string() });
    assert!(result.is_ok());
    
    let new_chain = result.unwrap();
    assert!(new_chain.contains::<TestModel1>());
    assert!(new_chain.contains::<TestModel2>());
}

#[test]
fn test_try_with_model_duplicate_error() {
    let chain = EmptyStorage::default()
        .with_model_unchecked(TestModel1 { value: 1 });
    
    // Adding same type should fail
    let result = chain.try_with_model(TestModel1 { value: 999 });
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    assert_eq!(err.type_name, std::any::type_name::<TestModel1>());
    assert!(err.to_string().contains("Duplicate type in chain"));
}

#[test]
#[should_panic(expected = "Duplicate type in chain")]
fn test_with_model_panics_on_duplicate() {
    let chain = EmptyStorage::default()
        .with_model_unchecked(TestModel1 { value: 1 });
    
    // This should panic
    let _bad_chain = chain.with_model(TestModel1 { value: 999 });
}

#[test]
fn test_with_model_success() {
    let chain = EmptyStorage::default()
        .with_model_unchecked(TestModel1 { value: 1 })
        .with_model(TestModel2 { name: "test".to_string() }); // Should work
    
    assert!(chain.contains::<TestModel1>());
    assert!(chain.contains::<TestModel2>());
    
    // Verify we can access both
    let model1: &TestModel1 = chain.get();
    let model2: &TestModel2 = chain.get();
    assert_eq!(model1.value, 1);
    assert_eq!(model2.name, "test");
}

#[test]
fn test_with_model_unchecked_allows_duplicates() {
    // Unchecked version should allow duplicates (but they become unusable)
    let chain = EmptyStorage::default()
        .with_model_unchecked(TestModel1 { value: 1 })
        .with_model_unchecked(TestModel1 { value: 999 }); // Should compile
    
    // Chain created successfully - duplicates allowed
    println!("Unchecked duplicate chain created - this is expected behavior");
    
    // But accessing becomes ambiguous:
    // let _model1: &TestModel1 = chain.get(); // Would fail to compile - ambiguous
    
    // The chain exists but is unusable for TestModel1 access
    assert!(std::mem::size_of_val(&chain) > 0);
}

#[test]
fn test_safe_chain_building_with_try() {
    // Build chain safely with proper error handling
    let result1 = EmptyStorage::default().try_with_model(TestModel1 { value: 1 }).unwrap();
    let result2 = result1.try_with_model(TestModel2 { name: "test".to_string() }).unwrap();
    let result3 = result2.try_with_model(TestModel3 { count: 10 });
    
    let chain = result3.expect("Chain building should succeed");
    assert!(chain.contains::<TestModel1>());
    assert!(chain.contains::<TestModel2>());
    assert!(chain.contains::<TestModel3>());
    
    // Try to add duplicate - should fail
    let duplicate_result = chain.try_with_model(TestModel1 { value: 999 });
    assert!(duplicate_result.is_err());
}

#[test]
fn test_safe_chain_building_with_panics() {
    // Build chain with panic on duplicates
    let chain = EmptyStorage::default()
        .with_model(TestModel1 { value: 1 })         // First one is fine
        .with_model(TestModel2 { name: "test".to_string() }) // Different type is fine
        .with_model(TestModel3 { count: 10 });       // Different type is fine
    
    assert!(chain.contains::<TestModel1>());
    assert!(chain.contains::<TestModel2>());
    assert!(chain.contains::<TestModel3>());
    
    // Verify all models are accessible
    let model1: &TestModel1 = chain.get();
    let model2: &TestModel2 = chain.get();
    let model3: &TestModel3 = chain.get();
    assert_eq!(model1.value, 1);
    assert_eq!(model2.name, "test");
    assert_eq!(model3.count, 10);
}

#[test]
fn test_nochain_methods() {
    // Test that NoChain has all the necessary methods
    let chain1 = EmptyStorage::default().with_model(TestModel1 { value: 1 });
    let chain2 = EmptyStorage::default().with_model_unchecked(TestModel1 { value: 1 });
    let chain3 = EmptyStorage::default().try_with_model(TestModel1 { value: 1 }).unwrap();
    
    // All should work the same for NoChain since it's empty
    assert!(chain1.contains::<TestModel1>());
    assert!(chain2.contains::<TestModel1>());
    assert!(chain3.contains::<TestModel1>());
}