// Basic integration tests for Syzygy public APIs

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use syzygy::{model::Model, prelude::*};
#[cfg(feature = "async")]
use tokio::time::{Duration, sleep};

/// Test model for integration tests
#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
    name: String,
    items: Vec<String>,
}

impl Default for TestModel {
    fn default() -> Self {
        Self {
            counter: 0,
            name: "test".to_string(),
            items: vec![],
        }
    }
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

/// Test resource
#[derive(Debug, Clone)]
struct TestResource {
    value: i32,
}

/// Global counter for testing
#[derive(Debug)]
struct GlobalCounter {
    count: AtomicU32,
}

impl GlobalCounter {
    fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
        }
    }

    fn increment(&self) -> u32 {
        self.count.fetch_add(1, Ordering::SeqCst)
    }

    fn get(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_builder_pattern() {
    let syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel {
            counter: 5,
            name: "builder_test".to_string(),
            items: vec!["initial".to_string()],
        })
        .resource(TestResource { value: 100 })
        .resource(GlobalCounter::new())
        .build();

    // Verify initial state
    assert_eq!(syzygy.model().counter, 5);
    assert_eq!(syzygy.model().name, "builder_test");
    assert_eq!(syzygy.model().items.len(), 1);

    // Verify resources are accessible
    let resource = syzygy.resource::<TestResource>();
    assert_eq!(resource.value, 100);

    let counter = syzygy.resource::<GlobalCounter>();
    assert_eq!(counter.get(), 0);
}

fn increment_counter(syzygy: &mut Syzygy<TestModel, ()>) {
    syzygy.model_mut().counter += 1;
}

fn use_resources(syzygy: &mut Syzygy<TestModel, ()>) {
    let test_res = syzygy.resource::<TestResource>();
    let counter = syzygy.resource::<GlobalCounter>();

    syzygy.model_mut().counter = test_res.value;
    counter.increment();
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_basic_effects() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .build();

    syzygy.dispatch_closure(increment_counter);
    syzygy.handle_effects();

    assert_eq!(syzygy.model().counter, 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_resource_access() {
    let syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 42 })
        .resource(GlobalCounter::new())
        .build();

    // Test resource access
    let resource = syzygy.resource::<TestResource>();
    assert_eq!(resource.value, 42);

    // Test cloned resource access (now user's responsibility to clone the Arc)
    let cloned = (*syzygy.resource::<TestResource>()).clone();
    assert_eq!(cloned.value, 42);

    // Test try_resource
    let maybe_resource = syzygy.try_resource::<TestResource>();
    assert!(maybe_resource.is_some());
    assert_eq!(maybe_resource.unwrap().value, 42);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_model_operations() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder().model(TestModel::default()).build();

    // Test query
    let initial_counter = syzygy.query(|model| model.counter);
    assert_eq!(initial_counter, 0);

    // Test update
    syzygy.update(|model| {
        model.counter = 42;
        model.name = "updated".to_string();
    });

    assert_eq!(syzygy.model().counter, 42);
    assert_eq!(syzygy.model().name, "updated");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_basic_async_task() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(GlobalCounter::new())
        .build();

    // Test async task dispatch
    syzygy.task(|async_ctx| async move {
        // Access resources from async context
        let counter = async_ctx.resource::<GlobalCounter>();
        counter.increment();

        // Dispatch effect to update model
        async_ctx.dispatch_closure(|s: &mut Syzygy<TestModel, ()>| {
            s.model_mut().counter = 100;
        });
    });

    // Wait for async task and process effects multiple times
    for _ in 0..5 {
        sleep(Duration::from_millis(10)).await;
        syzygy.handle_effects();
    }

    // Verify results
    assert_eq!(syzygy.model().counter, 100);
    let counter = syzygy.resource::<GlobalCounter>();
    assert_eq!(counter.get(), 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_multiple_resources() {
    let syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel::default())
        .resource(TestResource { value: 1 })
        .resource(GlobalCounter::new())
        .build();

    // Test accessing multiple resource types
    let test_res = syzygy.resource::<TestResource>();
    assert_eq!(test_res.value, 1);

    let counter = syzygy.resource::<GlobalCounter>();
    assert_eq!(counter.get(), 0);

    let mut syzygy = syzygy;
    syzygy.dispatch_closure(use_resources);
    syzygy.handle_effects();

    assert_eq!(syzygy.model().counter, 1);
    let counter = syzygy.resource::<GlobalCounter>();
    assert_eq!(counter.get(), 1);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_concurrent_access() {
    let syzygy = Arc::new(
        Syzygy::<TestModel, ()>::builder()
            .model(TestModel::default())
            .resource(GlobalCounter::new())
            .build(),
    );

    // Test direct concurrent resource access (not async tasks)
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let syzygy = Arc::clone(&syzygy);
            tokio::spawn(async move {
                // Direct resource access from concurrent contexts
                let counter = syzygy.resource::<GlobalCounter>();
                counter.increment();
            })
        })
        .collect();

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all increments were processed
    let counter = syzygy.resource::<GlobalCounter>();
    assert_eq!(counter.get(), 10);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_snapshot_consistency() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder().model(TestModel::default()).build();

    // Get initial snapshot
    let snapshot1 = syzygy.model().to_snapshot();
    assert_eq!(snapshot1.counter, 0);

    // Modify model
    syzygy.update(|model| model.counter = 10);

    // Original snapshot unchanged
    assert_eq!(snapshot1.counter, 0);

    // New snapshot reflects changes
    let snapshot2 = syzygy.model().to_snapshot();
    assert_eq!(snapshot2.counter, 10);
}
