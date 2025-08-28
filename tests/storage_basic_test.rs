#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args)]
//! Basic tests for simplified Storage functionality

use syzygy::storage::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct Model1 {
        pub value: u64,
        pub active: bool,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)] // Test model - fields used for storage verification
    struct Model2 {
        pub value: u64,
        pub name: String,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)] // Test model - fields used for storage verification
    struct Model3 {
        pub value: u64,
        pub count: i32,
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
    fn test_storage_with_multiple_models() {
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
}
