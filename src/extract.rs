//! Container field extraction traits for magic handlers
//!
//! This module provides the core traits for extracting fields from any container
//! (Model, Resources, etc.) in a type-safe way, enabling Axum-style magic function parameters.

/// Extract a value from an immutable reference to any container
/// 
/// This trait enables type-safe extraction of specific fields or computed values
/// from any container type. Used by magic handlers to automatically provide the right
/// parameters to handler functions.
pub trait FromContainer<T> {
    /// Extract Self from an immutable container reference
    fn from_container(container: &T) -> Self;
}

/// Extract a mutable value from a mutable reference to any container
/// 
/// This trait enables type-safe extraction of mutable references to specific
/// fields from any container type. Used by magic handlers when the handler function
/// needs to mutate part of the container.
pub trait FromContainerMut<T> {
    /// Extract Self from a mutable container reference
    fn from_container_mut(container: &mut T) -> Self;
}

// Implement for tuples to support multiple parameter extraction
impl<T, A> FromContainer<T> for (A,)
where
    A: FromContainer<T>,
{
    fn from_container(container: &T) -> Self {
        (A::from_container(container),)
    }
}

impl<T, A, B> FromContainer<T> for (A, B)
where
    A: FromContainer<T>,
    B: FromContainer<T>,
{
    fn from_container(container: &T) -> Self {
        (A::from_container(container), B::from_container(container))
    }
}

impl<T, A, B, C> FromContainer<T> for (A, B, C)
where
    A: FromContainer<T>,
    B: FromContainer<T>,
    C: FromContainer<T>,
{
    fn from_container(container: &T) -> Self {
        (A::from_container(container), B::from_container(container), C::from_container(container))
    }
}

impl<T, A, B, C, D> FromContainer<T> for (A, B, C, D)
where
    A: FromContainer<T>,
    B: FromContainer<T>,
    C: FromContainer<T>,
    D: FromContainer<T>,
{
    fn from_container(container: &T) -> Self {
        (A::from_container(container), B::from_container(container), C::from_container(container), D::from_container(container))
    }
}

// Mutable tuple impls require more careful handling since we can't borrow mutably multiple times
// For now, we'll implement for single parameter only
impl<T, A> FromContainerMut<T> for (A,)
where
    A: FromContainerMut<T>,
{
    fn from_container_mut(container: &mut T) -> Self {
        (A::from_container_mut(container),)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestModel {
        users: Vec<String>,
        counter: i32,
        name: String,
    }

    impl FromContainer<TestModel> for i32 {
        fn from_container(model: &TestModel) -> Self {
            model.counter
        }
    }

    impl FromContainer<TestModel> for String {
        fn from_container(model: &TestModel) -> Self {
            model.name.clone()
        }
    }

    #[test]
    fn test_from_container_single() {
        let model = TestModel {
            users: vec!["alice".to_string(), "bob".to_string()],
            counter: 42,
            name: "test".to_string(),
        };

        let counter: i32 = FromContainer::from_container(&model);
        assert_eq!(counter, 42);

        let name: String = FromContainer::from_container(&model);
        assert_eq!(name, "test");
    }

    #[test]
    fn test_from_container_tuple() {
        let model = TestModel {
            users: vec!["alice".to_string()],
            counter: 123,
            name: "tuple_test".to_string(),
        };

        let (counter, name): (i32, String) = FromContainer::from_container(&model);
        assert_eq!(counter, 123);
        assert_eq!(name, "tuple_test");
    }

}