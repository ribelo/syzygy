//! Basic tests for simplified Storage functionality

use syzygy::storage::*;

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

#[test]
fn test_storage_basic_functionality() {
    let storage = EmptyStorage
        .with_model(Model1 {
            value: 1,
            active: true,
        })
        .with_model(Model2 {
            value: 2,
            name: "test".to_string(),
        });

    let model1: &Model1 = storage.get();
    let model2: &Model2 = storage.get();

    assert_eq!(model1.value, 1);
    assert!(model1.active);
    assert_eq!(model2.value, 2);
    assert_eq!(model2.name, "test");
}

#[test]
fn test_storage_mutable_access() {
    let storage = EmptyStorage.with_model(Model1 {
        value: 10,
        active: false,
    });

    {
        let model: &mut Model1 = storage.get_mut();
        model.value = 20;
        model.active = true;
    }

    let model: &Model1 = storage.get();
    assert_eq!(model.value, 20);
    assert!(model.active);
}

#[test]
fn test_storage_multiple_models() {
    let storage = EmptyStorage
        .with_model(Model1 {
            value: 1,
            active: true,
        })
        .with_model(Model2 {
            value: 2,
            name: "hello".to_string(),
        })
        .with_model(Model3 {
            value: 3,
            count: 42,
        });

    let m1: &Model1 = storage.get();
    let m2: &Model2 = storage.get();
    let m3: &Model3 = storage.get();

    assert_eq!(m1.value, 1);
    assert_eq!(m2.name, "hello");
    assert_eq!(m3.count, 42);
}

#[test]
fn test_storage_contains_trait() {
    let storage = EmptyStorage.with_model(Model1 {
        value: 1,
        active: true,
    });

    assert!(storage.contains::<Model1>());
    assert!(!storage.contains::<Model2>());
}
