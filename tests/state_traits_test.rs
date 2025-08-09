// Test for unified state access traits

use syzygy::{model::Model, prelude::*};

#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
    name: String,
}

impl Default for TestModel {
    fn default() -> Self {
        Self {
            counter: 0,
            name: "test".to_string(),
        }
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

#[derive(Debug, Clone)]
struct AnotherResource {
    data: String,
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_state_access_trait() {
    let syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .resource(AnotherResource {
            data: "hello".to_string(),
        })
        .build();

    // Test query_state method
    let result = syzygy.query_state(|model, resources| {
        let test_res = resources.get::<TestResource>().unwrap();
        (model.counter, test_res.value)
    });
    assert_eq!(result, (0, 42));

    // Test with_model_and_resource method
    let result = syzygy.with_model_and_resource::<TestResource, _, _>(|model, resource| {
        format!("{}:{}", model.name, resource.value)
    });
    assert_eq!(result, "test:42");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_state_modify_trait() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 10 })
        .build();

    // Test replace_resource_and_update
    let old_value =
        syzygy.replace_resource_and_update(TestResource { value: 100 }, |model, old_resource| {
            model.counter = old_resource.as_ref().map(|r| r.value).unwrap_or(0);
            old_resource.map(|r| r.value)
        });

    assert_eq!(old_value, Some(10));
    assert_eq!(syzygy.model().counter, 10);

    // Verify new resource is in place
    let new_resource = syzygy.resource::<TestResource>();
    assert_eq!(new_resource.value, 100);

    // Test update_model_then_access_resource
    let result = syzygy.update_model_then_access_resource::<TestResource, _, _, _>(
        |model| {
            model.name = "updated".to_string();
        },
        |resource| resource.value * 2,
    );

    assert_eq!(result, 200);
    assert_eq!(syzygy.model().name, "updated");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_state_traits_integration() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 5 })
        .resource(AnotherResource {
            data: "world".to_string(),
        })
        .build();

    // Complex operation using multiple state trait methods
    let initial_state = syzygy.query_state(|model, resources| {
        let test_res = resources.get::<TestResource>().unwrap();
        let another_res = resources.get::<AnotherResource>().unwrap();
        format!("{}:{}:{}", model.name, test_res.value, another_res.data)
    });
    assert_eq!(initial_state, "test:5:world");

    // Update state using replace and model update
    syzygy.replace_resource_and_update(
        AnotherResource {
            data: "universe".to_string(),
        },
        |model, old_resource| {
            if let Some(old) = old_resource {
                model.name = format!("{}_{}", model.name, old.data);
            }
        },
    );

    // Verify final state
    let final_state = syzygy.query_state(|model, resources| {
        let another_res = resources.get::<AnotherResource>().unwrap();
        format!("{}:{}", model.name, another_res.data)
    });
    assert_eq!(final_state, "test_world:universe");
}
