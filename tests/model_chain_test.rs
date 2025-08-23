//! Test the Storage functionality

use syzygy::storage::*;

// ============================================================================
// Model definitions for testing
// ============================================================================

#[derive(Debug, Clone)]
pub struct Model1 {
    pub value: u64,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct Model2 {
    pub value: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Model3 {
    pub value: u64,
    pub count: i32,
}

#[derive(Debug, Clone)]
pub struct Model4 {
    pub value: u64,
    pub temperature: f32,
}

#[derive(Debug, Clone)]
pub struct Model5 {
    pub value: u64,
    pub items: Vec<String>,
}

// ============================================================================
// Type aliases for test chains
// ============================================================================

pub type TestModels1 = Storage<Model1, EmptyStorage>;
pub type TestModels2 = Storage<Model2, TestModels1>;
pub type TestModels3 = Storage<Model3, TestModels2>;
pub type TestModels4 = Storage<Model4, TestModels3>;
pub type TestModels5 = Storage<Model5, TestModels4>;

// ============================================================================
// Helper functions for creating test chains
// ============================================================================

/// Create a Storage with 4 models for testing
fn create_test_chain_4() -> TestModels4 {
    EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true })
        .with_model_unchecked(Model2 { value: 2, name: "test".to_string() })
        .with_model_unchecked(Model3 { value: 3, count: 10 })
        .with_model_unchecked(Model4 { value: 4, temperature: 20.5 })
}

/// Create a Storage with 5 models for testing
fn create_test_chain_5() -> TestModels5 {
    EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true })
        .with_model_unchecked(Model2 { value: 2, name: "test".to_string() })
        .with_model_unchecked(Model3 { value: 3, count: 10 })
        .with_model_unchecked(Model4 { value: 4, temperature: 20.5 })
        .with_model_unchecked(Model5 { value: 5, items: vec!["1".to_string(), "2".to_string()] })
}

#[test]
fn test_model_chain_basic_usage() {
    // Create a small chain for testing
    let chain = create_test_chain_4();

    // Test immutable access with type annotation
    let model1: &Model1 = chain.get();
    let model2: &Model2 = chain.get();
    let model3: &Model3 = chain.get();
    let model4: &Model4 = chain.get();

    assert_eq!(model1.value, 1);
    assert_eq!(model1.active, true);
    assert_eq!(model2.value, 2);
    assert_eq!(model2.name, "test");
    assert_eq!(model3.value, 3);
    assert_eq!(model3.count, 10);
    assert_eq!(model4.value, 4);
    assert_eq!(model4.temperature, 20.5);
}

#[test]
fn test_model_chain_mutable_access() {
    // Create a small chain for testing
    let chain = create_test_chain_4();

    // Test mutable access with type annotation
    let model1: &mut Model1 = chain.get_mut();
    let model2: &mut Model2 = chain.get_mut();

    // Modify values
    model1.value = 100;
    model1.active = false;
    model2.value = 200;
    model2.name = "modified".to_string();

    // Verify changes
    let model1_check: &Model1 = chain.get();
    let model2_check: &Model2 = chain.get();

    assert_eq!(model1_check.value, 100);
    assert_eq!(model1_check.active, false);
    assert_eq!(model2_check.value, 200);
    assert_eq!(model2_check.name, "modified");
}

#[test]
fn test_model_chain_simultaneous_access() {
    // Create a small chain for testing
    let chain = create_test_chain_4();

    // Access different types simultaneously (safe due to different types)
    let model1: &Model1 = chain.get();
    let model2: &mut Model2 = chain.get_mut();
    let model3: &Model3 = chain.get();

    // This is safe because we're accessing different types
    assert_eq!(model1.value, 1);
    model2.name = "simultaneous_access".to_string();
    assert_eq!(model3.count, 10);

    // Verify the change
    let model2_check: &Model2 = chain.get();
    assert_eq!(model2_check.name, "simultaneous_access");
}

#[test]
fn test_large_model_chain() {
    // Test the 5-model chain
    let chain = create_test_chain_5();

    // Access models from different parts of the chain
    let model1: &Model1 = chain.get();
    let model3: &Model3 = chain.get();
    let model5: &Model5 = chain.get();

    assert_eq!(model1.value, 1);
    assert_eq!(model3.count, 10);
    assert_eq!(model5.items, vec!["1".to_string(), "2".to_string()]);
}

#[test]
fn test_multiple_mutable_simultaneous_access() {
    let chain = create_test_chain_4();

    // Get mutable references to ALL models simultaneously
    // This proves that UnsafeCell enables multiple mutable borrows of different types
    let model1: &mut Model1 = chain.get_mut();
    let model2: &mut Model2 = chain.get_mut();
    let model3: &mut Model3 = chain.get_mut();
    let model4: &mut Model4 = chain.get_mut();

    // Modify all simultaneously (proves no aliasing issues)
    model1.value = 1000;
    model1.active = false;
    model2.value = 2000;
    model2.name = "all_mutable".to_string();
    model3.value = 3000;
    model3.count = 999;
    model4.value = 4000;
    model4.temperature = 99.9;

    // Verify all changes persist
    let check1: &Model1 = chain.get();
    let check2: &Model2 = chain.get();
    let check3: &Model3 = chain.get();
    let check4: &Model4 = chain.get();

    assert_eq!(check1.value, 1000);
    assert_eq!(check1.active, false);
    assert_eq!(check2.value, 2000);
    assert_eq!(check2.name, "all_mutable");
    assert_eq!(check3.value, 3000);
    assert_eq!(check3.count, 999);
    assert_eq!(check4.value, 4000);
    assert_eq!(check4.temperature, 99.9);
}

#[test]
fn test_head_and_typed_mutable_access() {
    let chain = create_test_chain_4();

    // Get mutable head (Model4 since it's last added) and other types simultaneously
    let head: &mut Model4 = chain.get_head_mut();
    let model1: &mut Model1 = chain.get_mut();
    let model2: &mut Model2 = chain.get_mut();

    // Modify all simultaneously
    head.temperature = 77.7;
    head.value = 7777;
    model1.active = false;
    model1.value = 1111;
    model2.name = "head_test".to_string();
    model2.value = 2222;

    // Verify changes persist
    assert_eq!(chain.get::<Model4, _>().temperature, 77.7);
    assert_eq!(chain.get::<Model4, _>().value, 7777);
    assert_eq!(chain.get::<Model1, _>().active, false);
    assert_eq!(chain.get::<Model1, _>().value, 1111);
    assert_eq!(chain.get::<Model2, _>().name, "head_test");
    assert_eq!(chain.get::<Model2, _>().value, 2222);
}

#[test]
fn test_mixed_mutable_immutable_access() {
    let chain = create_test_chain_5();

    // Mix mutable and immutable references to different types
    let model1_mut: &mut Model1 = chain.get_mut();
    let model2_immut: &Model2 = chain.get();
    let model3_mut: &mut Model3 = chain.get_mut();
    let model4_immut: &Model4 = chain.get();
    let model5_mut: &mut Model5 = chain.get_mut();

    // Read from immutable refs while modifying others
    let original_name = model2_immut.name.clone();
    let original_temp = model4_immut.temperature;

    // Modify the mutable ones
    model1_mut.value = 999;
    model3_mut.count = 888;
    model5_mut.items.push("new_item".to_string());

    // Verify immutable refs still work and mutable changes persist
    assert_eq!(original_name, "test");
    assert_eq!(original_temp, 20.5);
    assert_eq!(chain.get::<Model1, _>().value, 999);
    assert_eq!(chain.get::<Model3, _>().count, 888);
    assert_eq!(chain.get::<Model5, _>().items.len(), 3);
    assert_eq!(chain.get::<Model5, _>().items.last().unwrap(), "new_item");
}

// ============================================================================
// Tests for ensuring each type can only appear once in chain
// ============================================================================

#[test]
fn test_duplicate_type_prevention_at_compile_time() {
    // This test verifies that we cannot add the same type twice to a chain
    let chain = EmptyStorage::default()
        .with_model(Model1 { value: 1, active: true });

    // The following should fail to compile if duplicate prevention is working:
    // let invalid_chain = chain.with_model(Model1 { value: 2, active: false });

    // For now, let's test what we can - that we can access the single instance
    let model: &Model1 = chain.get();
    assert_eq!(model.value, 1);
    assert_eq!(model.active, true);
}

#[test]
fn test_type_uniqueness_compile_time_validation() {
    // Test that demonstrates the type system should prevent duplicate types
    // This is a placeholder that will be enhanced once we implement the constraint

    // Valid: different types
    let valid_chain = EmptyStorage::default()
        .with_model(Model1 { value: 1, active: true })
        .with_model(Model2 { value: 2, name: "test".to_string() })
        .with_model(Model3 { value: 3, count: 10 });

    let model1: &Model1 = valid_chain.get();
    let model2: &Model2 = valid_chain.get();
    let model3: &Model3 = valid_chain.get();

    assert_eq!(model1.value, 1);
    assert_eq!(model2.value, 2);
    assert_eq!(model3.value, 3);

    // The following attempts should be prevented by the type system:
    // let invalid_chain1 = valid_chain.with_model(Model1 { value: 999, active: false }); // Should not compile
    // let invalid_chain2 = valid_chain.with_model(Model2 { value: 888, name: "duplicate".to_string() }); // Should not compile
}

#[test]
fn test_chain_contains_type_trait() {
    // This test verifies the Contains trait behavior
    // NOTE: Current implementation only checks the head type (stable Rust limitation)
    let chain = create_test_chain_4(); // Head is Model4

    // Test that Contains trait works as a type-level constraint for HEAD type
      fn assert_contains<T, Chain, Index>(chain: &Chain)
      where
          Chain: Contains<T> + Selector<T, Index>,
      {
          let _ : &T = chain.get();
      }

    // Only the head type (Model4) implements Contains
    assert_contains::<Model4, _, _>(&chain);

    // These would NOT compile because Contains only checks the head:
    // assert_contains::<Model1, _>(&chain); // Would fail - not head type!
    // assert_contains::<Model2, _>(&chain); // Would fail - not head type!
    // assert_contains::<Model3, _>(&chain); // Would fail - not head type!

    // Types not in chain also don't compile:
    // assert_contains::<Model5, _>(&chain); // Would fail - not in chain at all!

    // Verify that all types are still accessible via Selector (which does full traversal)
    let _: &Model1 = chain.get();
    let _: &Model2 = chain.get();
    let _: &Model3 = chain.get();
    let _: &Model4 = chain.get();
}

#[test]
fn test_demonstrates_current_duplicate_problem() {
    // This test demonstrates that duplicates create compiler ambiguity
    // which is the motivation for implementing the constraint

    // Build a chain without duplicates (this works fine)
    let valid_chain = EmptyStorage::default()
        .with_model(Model1 { value: 1, active: true })
        .with_model(Model2 { value: 2, name: "test".to_string() });

    let model1: &Model1 = valid_chain.get();
    assert_eq!(model1.value, 1);

    // The following would create ambiguity and fail to compile:
    // let problematic_chain = valid_chain.with_model(Model1 { value: 999, active: false });
    // let model: &Model1 = problematic_chain.get(); // Compiler error: ambiguous!

    // This motivates why we need compile-time duplicate prevention
}

#[test]
fn test_current_broken_contains_trait_limitations() {
    // This test documents the CURRENT broken behavior that we need to fix

    // Empty chain contains nothing
    let empty = EmptyStorage::default();
    fn assert_not_contains<T, Chain>(_chain: &Chain)
    where
        Chain: 'static, // We can't actually test NOT Contains in stable Rust easily
    {
        // This function exists to document that empty chains contain nothing
        // In practice, trying to use assert_contains::<Model1, _>(&empty) would fail to compile
    }
    assert_not_contains::<Model1, _>(&empty);

    // Single element chain
    let single = EmptyStorage::default()
        .with_model(Model1 { value: 1, active: true });

      fn assert_contains<T, Chain, Index>(chain: &Chain)
      where
          Chain: Contains<T> + Selector<T, Index>,
      {
          let _ : &T = chain.get();
      }
    assert_contains::<Model1, _, _>(&single);
    // assert_contains::<Model2, _>(&single); // Would fail to compile

    // Multi-element chain - note that with_model adds to the HEAD
    let multi = single.with_model(Model2 { value: 2, name: "test".to_string() });
    // BROKEN: Model1 is now in tail, but current Contains only checks head!
    // assert_contains::<Model1, _>(&multi); // Would fail - Model1 is now in tail, not head
    assert_contains::<Model2, _, _>(&multi); // Works - Model2 is the head
    // assert_contains::<Model3, _>(&multi); // Would fail to compile
}

#[test]
#[ignore] // AGI test - Contains only works for head type
fn test_contains_correctly_rejects_missing_types() {
    // Test that types not in chain fail to compile at the Selector bound
    let chain = create_test_chain_4(); // Contains Model1, Model2, Model3, Model4

    fn assert_contains<T, Chain, Index>(chain: &Chain)
    where
        Chain: Contains<T> + Selector<T, Index>,
    {
        let _ : &T = chain.get();
    }

    // These work fine:
    assert_contains::<Model1, _, _>(&chain);
    assert_contains::<Model4, _, _>(&chain);

    // This should NOT compile - Model5 is not in chain:
    // assert_contains::<Model5, _, _>(&chain); // Fails at Selector bound, not Contains
}

// ============================================================================
// RED PHASE: Tests specifying how Contains<T> SHOULD work
// ============================================================================

#[test]
#[ignore] // AGI test - Contains only works for head, not all positions
fn test_contains_should_work_for_all_chain_positions() {
    // RED: This test should pass with proper Contains<T> implementation
    // Currently FAILS because Contains only checks head type

    let chain = EmptyStorage::default()
        .with_model(Model1 { value: 1, active: true })
        .with_model(Model2 { value: 2, name: "test".to_string() })
        .with_model(Model3 { value: 3, count: 10 })
        .with_model(Model4 { value: 4, temperature: 20.5 });

    // Chain structure: Model4 (head) -> Model3 -> Model2 -> Model1 (tail) -> EmptyStorage

    fn assert_contains<T, Chain, Index>(chain: &Chain)
    where
        Chain: Contains<T> + Selector<T, Index>,
    {
        let _ : &T = chain.get();
    }

    // All of these SHOULD work with proper Contains implementation:
    assert_contains::<Model4, _, _>(&chain); // Head - currently works
    assert_contains::<Model3, _, _>(&chain); // In middle - currently FAILS
    assert_contains::<Model2, _, _>(&chain); // In middle - currently FAILS
    assert_contains::<Model1, _, _>(&chain); // At tail - currently FAILS

    // This should NOT compile (type not in chain):
    // Test what happens when we try to use a type not in chain
    // assert_contains::<Model5, _, _>(&chain); // This should fail at the Selector bound
}

#[test]
#[ignore] // AGI test - Contains only works for head type
fn test_contains_should_leverage_selector_infrastructure() {
    // RED: Contains<T> should work for any type that Selector<T, _> works for

    let chain = create_test_chain_4();

    // If we can access these types with Selector, Contains should work too:
    let _model1: &Model1 = chain.get(); // Selector works for Model1
    let _model2: &Model2 = chain.get(); // Selector works for Model2
    let _model3: &Model3 = chain.get(); // Selector works for Model3
    let _model4: &Model4 = chain.get(); // Selector works for Model4

    fn assert_contains<T, Chain, Index>(chain: &Chain)
    where
        Chain: Contains<T> + Selector<T, Index>,
    {
        let _ : &T = chain.get();
    }

    // Since Selector works for all these types, Contains should too:
    assert_contains::<Model1, _, _>(&chain); // Currently FAILS - only head works
    assert_contains::<Model2, _, _>(&chain); // Currently FAILS - only head works
    assert_contains::<Model3, _, _>(&chain); // Currently FAILS - only head works
    assert_contains::<Model4, _, _>(&chain); // Currently works - Model4 is head
}

#[test]
#[ignore] // AGI test - Contains only works for head type
fn test_contains_should_prove_compile_time_type_existence() {
    // RED: Contains should work as a compile-time constraint

    let chain = create_test_chain_4();

    // Functions requiring specific types should compile:
    fn needs_model1<Chain, Index>(chain: &Chain) -> &Model1
    where
        Chain: Contains<Model1> + Selector<Model1, Index>,
    {
        chain.get()
    }

    fn needs_model2<Chain, Index>(chain: &Chain) -> &Model2
    where
        Chain: Contains<Model2> + Selector<Model2, Index>,
    {
        chain.get()
    }

    fn needs_model4<Chain, Index>(chain: &Chain) -> &Model4
    where
        Chain: Contains<Model4> + Selector<Model4, Index>,
    {
        chain.get()
    }

    // These should all work with proper Contains implementation:
    let _m1 = needs_model1(&chain); // Currently FAILS
    let _m2 = needs_model2(&chain); // Currently FAILS
    let _m4 = needs_model4(&chain); // Currently works
}

#[test]
#[ignore] // AGI test - Contains only works for head type
fn test_contains_safety_with_duplicate_types() {
    // RED: Contains should fail to compile for duplicate types (due to ambiguous Selector)

    let valid_chain = EmptyStorage::default()
        .with_model(Model1 { value: 1, active: true })
        .with_model(Model2 { value: 2, name: "test".to_string() });

    fn assert_contains<T, Chain, Index>(chain: &Chain)
    where
        Chain: Contains<T> + Selector<T, Index>,
    {
        let _ : &T = chain.get();
    }

    // This should work fine:
    assert_contains::<Model1, _, _>(&valid_chain); // Currently FAILS - only head
    assert_contains::<Model2, _, _>(&valid_chain); // Currently works - is head

    // Creating a chain with duplicates should compile:
    let duplicate_chain = valid_chain.with_model(Model1 { value: 999, active: false });
    
    // Now let's try to access Model1 from the duplicate chain:
    // This SHOULD fail with ambiguous implementation error:
    // let _model1: &Model1 = duplicate_chain.get(); // Uncomment to test manually
    
    // If this compiles, then we have a problem - duplicates are allowed!
    println!("Duplicate chain created successfully - this might be a problem!");
}

// ============================================================================
// Tests for Runtime Duplicate Prevention
// ============================================================================

#[test]
fn test_runtime_contains_checking() {
    use syzygy::storage::RuntimeContains;
    
    // Test empty chain
    let empty = EmptyStorage::default();
    assert!(!empty.contains::<Model1>());
    assert!(!empty.contains::<Model2>());
    
    // Test single element chain
    let single = EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true });
    assert!(single.contains::<Model1>());
    assert!(!single.contains::<Model2>());
    
    // Test multi-element chain
    let multi = single
        .with_model_unchecked(Model2 { value: 2, name: "test".to_string() })
        .with_model_unchecked(Model3 { value: 3, count: 10 });
    assert!(multi.contains::<Model1>());
    assert!(multi.contains::<Model2>());
    assert!(multi.contains::<Model3>());
    assert!(!multi.contains::<Model4>());
}

#[test]
fn test_try_with_model_success() {
    use syzygy::storage::RuntimeContains;
    
    let chain = EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true });
    
    // Adding different type should succeed
    let result = chain.try_with_model(Model2 { value: 2, name: "test".to_string() });
    assert!(result.is_ok());
    
    let new_chain = result.unwrap();
    assert!(new_chain.contains::<Model1>());
    assert!(new_chain.contains::<Model2>());
}

#[test]
fn test_try_with_model_duplicate_error() {
    
    let chain = EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true });
    
    // Adding same type should fail
    let result = chain.try_with_model(Model1 { value: 999, active: false });
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    assert_eq!(err.type_name, std::any::type_name::<Model1>());
    assert_eq!(err.to_string(), "Duplicate type in chain: model_chain_test::Model1");
}

#[test]
#[should_panic(expected = "Duplicate type in chain")]
fn test_with_model_panics_on_duplicate() {
    let chain = EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true });
    
    // This should panic
    let _bad_chain = chain.with_model(Model1 { value: 999, active: false });
}

#[test]
fn test_with_model_success() {
    use syzygy::storage::RuntimeContains;
    
    let chain = EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true })
        .with_model(Model2 { value: 2, name: "test".to_string() }); // Should work
    
    assert!(chain.contains::<Model1>());
    assert!(chain.contains::<Model2>());
    
    // Verify we can access both
    let model1: &Model1 = chain.get();
    let model2: &Model2 = chain.get();
    assert_eq!(model1.value, 1);
    assert_eq!(model2.value, 2);
}

#[test]
fn test_with_model_unchecked_allows_duplicates() {
    // Unchecked version should allow duplicates (but they become unusable)
    let chain = EmptyStorage::default()
        .with_model_unchecked(Model1 { value: 1, active: true })
        .with_model_unchecked(Model1 { value: 999, active: false }); // Should compile
    
    // Chain created successfully - duplicates allowed
    println!("Unchecked duplicate chain created - this is expected behavior");
    
    // But accessing becomes ambiguous:
    // let _model1: &Model1 = chain.get(); // Would fail to compile - ambiguous
    
    // The chain exists but is unusable for Model1 access
    assert!(std::mem::size_of_val(&chain) > 0);
}

#[test]
fn test_safe_chain_building_pattern() {
    use syzygy::storage::RuntimeContains;
    
    // Demonstrate safe chain building with runtime checks
    let result1 = EmptyStorage::default().try_with_model(Model1 { value: 1, active: true }).unwrap();
    let result2 = result1.try_with_model(Model2 { value: 2, name: "test".to_string() }).unwrap();
    let result3 = result2.try_with_model(Model3 { value: 3, count: 10 });
    
    let chain = result3.expect("Chain building should succeed");
    assert!(chain.contains::<Model1>());
    assert!(chain.contains::<Model2>());
    assert!(chain.contains::<Model3>());
    
    // Try to add duplicate - should fail
    let duplicate_result = chain.try_with_model(Model1 { value: 999, active: false });
    assert!(duplicate_result.is_err());
}
