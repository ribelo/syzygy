// Test for new resource modification features

use std::sync::Mutex;
use syzygy::{model::Model, prelude::*};

#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
}

impl Default for TestModel {
    fn default() -> Self {
        Self { counter: 0 }
    }
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

#[derive(Debug, Clone)]
struct TestResource {
    value: i32,
}

#[derive(Debug)]
struct MutableResource {
    data: Mutex<Vec<String>>,
}

impl MutableResource {
    fn new() -> Self {
        Self {
            data: Mutex::new(vec!["initial".to_string()]),
        }
    }

    fn add_item(&self, item: String) {
        let mut data = self.data.lock().unwrap();
        data.push(item);
    }

    fn get_items(&self) -> Vec<String> {
        self.data.lock().unwrap().clone()
    }
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_update_resource() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .build();

    // Test resource access and computation - resources are now accessed as Arc<T>
    let result = syzygy.with_resource::<TestResource, _, _>(|resource| {
        resource.value * 2
    });
    
    assert_eq!(result, 84);

    // Original resource unchanged (immutable)
    let resource = syzygy.resource::<TestResource>();
    assert_eq!(resource.value, 42);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_replacement() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // Set a resource (replaces add_resource)
    let old = syzygy.set_resource(TestResource { value: 10 });
    assert!(old.is_none()); // No previous resource
    let resource = syzygy.resource::<TestResource>();
    assert_eq!(resource.value, 10);

    // Replace it (set_resource returns the old value)
    let old = syzygy.set_resource(TestResource { value: 20 });
    assert!(old.is_some());
    assert_eq!(old.unwrap().value, 10);

    // Verify replacement
    let new_resource = syzygy.resource::<TestResource>();
    assert_eq!(new_resource.value, 20);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_batch_resource_operations() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 1 })
        .resource(MutableResource::new())
        .build();

    // Test that resources are accessible individually
    // (Batch operations removed - resources should be accessed one at a time)
    let test_resource = syzygy.try_resource::<TestResource>();
    let mutable_resource = syzygy.try_resource::<MutableResource>();
    
    assert!(test_resource.is_some());
    assert!(mutable_resource.is_some());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_removal() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .build();

    // Verify resource exists
    let resource = syzygy.try_resource::<TestResource>();
    assert!(resource.is_some());

    // Remove resource
    let removed = syzygy.remove_resource::<TestResource>();
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().value, 42);

    // Verify resource is gone
    let missing = syzygy.try_resource::<TestResource>();
    assert!(missing.is_none());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_mutable_resource_pattern() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(MutableResource::new())
        .build();

    // Access mutable resource (using interior mutability pattern)
    let mutable_res = syzygy.resource::<MutableResource>();
    
    // Verify initial state
    let initial_items = mutable_res.get_items();
    assert_eq!(initial_items, vec!["initial"]);

    // Modify through interior mutability
    mutable_res.add_item("new_item".to_string());

    // Verify modification
    let updated_items = mutable_res.get_items();
    assert_eq!(updated_items, vec!["initial", "new_item"]);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_expect_resource() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .build();

    // Test expect_resource with existing resource
    let resource = syzygy.expect_resource::<TestResource>("should exist");
    assert_eq!(resource.value, 42);

    // Test expect_resource with cloning (now user's responsibility)
    let cloned = (*syzygy.expect_resource::<TestResource>("should exist")).clone();
    assert_eq!(cloned.value, 42);
}

#[cfg(feature = "async")]
#[tokio::test]
#[should_panic(expected = "Resource of type resource_modification_test::TestResource not found")]
async fn test_resource_panic_with_better_message() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // This should panic with a descriptive error message
    let _resource = syzygy.resource::<TestResource>();
}

#[cfg(feature = "async")]
#[tokio::test]
#[should_panic(expected = "Resource of type resource_modification_test::TestResource not found: custom error message")]
async fn test_expect_resource_panic_with_custom_message() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // This should panic with a custom error message
    let _resource = syzygy.expect_resource::<TestResource>("custom error message");
}