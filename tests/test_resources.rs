use syzygy::prelude::*;

#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

#[derive(Debug, Clone)]
struct TestResource {
    name: String,
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test]
async fn test_resources() {
    let model = TestModel { counter: 0 };
    let test_resource = TestResource {
        name: "test_str".to_string(),
    };
    let syzygy = Syzygy::builder()
        .model(model)
        .resource(test_resource)
        .build();

    // Test accessing existing TestResource
    assert_eq!(syzygy.resource::<TestResource>().name, "test_str");

    // Test try_resource for existing resources
    let test_resource = syzygy.try_resource::<TestResource>();
    assert!(test_resource.is_some());
    assert_eq!(test_resource.unwrap().name, "test_str");

    // Test cloned access (now user's responsibility to clone the Arc)
    let cloned = (*syzygy.resource::<TestResource>()).clone();
    assert_eq!(cloned.name, "test_str");
}
