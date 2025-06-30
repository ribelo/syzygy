// Test for function-specific error handling

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

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_not_found_error() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // Test get_resource with missing resource
    let result = syzygy.get_resource::<TestResource>();
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    assert_eq!(err.type_name, std::any::type_name::<TestResource>());
    assert_eq!(err.type_id, std::any::TypeId::of::<TestResource>());
    assert!(err.to_string().contains("TestResource"));
    assert!(err.to_string().contains("not found"));
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_found_success() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .build();

    // Test get_resource with existing resource
    let result = syzygy.get_resource::<TestResource>();
    assert!(result.is_ok());
    
    let resource = result.unwrap();
    assert_eq!(resource.value, 42);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_cloned_error() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // Test get_resource_cloned with missing resource
    let result = syzygy.get_resource_cloned::<TestResource>();
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    assert!(err.to_string().contains("TestResource"));
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_cloned_success() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 100 })
        .build();

    // Test get_resource_cloned with existing resource
    let result = syzygy.get_resource_cloned::<TestResource>();
    assert!(result.is_ok());
    
    let resource = result.unwrap();
    assert_eq!(resource.value, 100);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_error_composability() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // Test that errors can be properly handled in functions
    fn try_get_resource(syzygy: &Syzygy<TestModel>) -> Result<TestResource, Box<dyn std::error::Error>> {
        let resource = syzygy.get_resource_cloned::<TestResource>()?;
        Ok(resource)
    }

    let result = try_get_resource(&syzygy);
    assert!(result.is_err());
    
    // Verify the error can be downcasted to the specific type
    let err = result.unwrap_err();
    assert!(err.downcast_ref::<ResourceNotFoundError>().is_some());
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_error_vs_option_apis() {
    let syzygy = Syzygy::builder()
        .model(TestModel::default())
        .build();

    // Option API (try_resource) returns None
    let option_result = syzygy.try_resource::<TestResource>();
    assert!(option_result.is_none());

    // Error API (get_resource) returns specific error
    let error_result = syzygy.get_resource::<TestResource>();
    assert!(error_result.is_err());

    // Both represent the same condition but with different handling approaches
    assert!(option_result.is_none() && error_result.is_err());
}