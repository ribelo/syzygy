//! Context-based extractors for magic handlers
//!
//! This module provides traits for extracting values from EventContext and EffectContext
//! to enable Axum-style magic parameter injection in handler functions.
//!
//! The extractors work with the current Syzygy EventContext and EffectContext designs,
//! providing type-safe access to models and resources through the Storage system.

use std::ops::{Deref, DerefMut};

use crate::async_context::EffectContext;
use crate::event_context::EventContext;
use crate::storage::Selector;

/// Extract a value from an EventContext
///
/// This trait enables type-safe extraction of values from EventContext,
/// which provides access to model storage. Extractors typically return
/// wrapper types that hold references to avoid lifetime issues.
///
/// # Example
/// ```rust,ignore
/// struct UserCount(i32);
///
/// impl<Event, Effect, Storage> FromEventContext<Event, Effect, Storage> for UserCount
/// where
///     Storage: Selector<UserModel, syzygy::storage::storage::Here>,
/// {
///     fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
///         UserCount(ctx.model::<UserModel, syzygy::storage::storage::Here>().count)
///     }
/// }
/// ```
pub trait FromEventContext<'ctx, Event, Effect, Storage, Index> {
    /// Extract Self from an EventContext
    fn from_context(ctx: &'ctx EventContext<Event, Effect, Storage>) -> Self;
}

/// Extract a value from an EffectContext
///
/// This trait enables type-safe extraction of values from EffectContext,
/// which provides access to resources, event sending, and task spawning.
///
/// # Example
/// ```rust,ignore
/// struct DatabaseUrl(String);
///
/// impl<Event, Resources> FromEffectContext<Event, Resources> for DatabaseUrl
/// where
///     Resources: Selector<DatabaseResource, syzygy::storage::storage::Here>,
/// {
///     fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
///         DatabaseUrl(ctx.resource::<DatabaseResource, syzygy::storage::storage::Here>().url.clone())
///     }
/// }
/// ```
pub trait FromEffectContext<'ctx, Event, Resources, Index> {
    /// Extract Self from an EffectContext
    fn from_context(ctx: &'ctx EffectContext<Event, Resources>) -> Self;
}

// ============================================================================
// Common Extractor Implementations
// ============================================================================

/// Wrapper for extracting a borrowed model from EventContext
///
/// This provides an ergonomic way to extract a reference to a model.
/// The underlying model is NOT cloned.
///
/// # Example
/// ```rust,ignore
/// fn handle_event(event: MyEvent, model: ModelRef<'_, UserModel>) -> Command<Event, Effect> {
///     println!("Current user: {}", model.0.name);
///     Command::none()
/// }
/// ```
#[derive(Debug)]
pub struct ModelRef<'a, T>(pub &'a T);

impl<'a, T> Deref for ModelRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

#[derive(Debug)]
pub struct ModelMut<'a, T>(pub &'a mut T);

impl<'a, T> Deref for ModelMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
impl<'a, T> DerefMut for ModelMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}
/// Wrapper for extracting a cloned resource from EffectContext
///
/// This provides an ergonomic way to extract an owned clone of a resource.
/// Unlike models, resources can be cheaply cloned (e.g., Arcs or handles).
///
/// # Example
/// ```rust,ignore
/// fn handle_effect(effect: MyEffect, db: Resource<DatabaseResource>) -> impl Future<Output = ()> {
///     async move {
///         println!("Using database: {}", db.0.url);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Resource<T>(pub T);

// Default implementation for ModelRef<'a, T> - borrows from context (no clone)
impl<'ctx, Event, Effect, Storage, T, I> FromEventContext<'ctx, Event, Effect, Storage, I>
    for ModelRef<'ctx, T>
where
    Storage: Selector<T, I>,
{
    fn from_context(ctx: &'ctx EventContext<Event, Effect, Storage>) -> Self {
        ModelRef(ctx.model::<T, I>())
    }
}

// Default implementation for ModelMut<'a, T> - borrows from context (no clone)
impl<'ctx, Event, Effect, Storage, T, I> FromEventContext<'ctx, Event, Effect, Storage, I>
    for ModelMut<'ctx, T>
where
    Storage: Selector<T, I>,
{
    fn from_context(ctx: &'ctx EventContext<Event, Effect, Storage>) -> Self {
        ModelMut(ctx.model_mut::<T, I>())
    }
}

// Default implementation for Resource<T> - clones from context
impl<'ctx, Event, Resources, T, I> FromEffectContext<'ctx, Event, Resources, I> for Resource<T>
where
    Resources: Selector<T, I>,
    T: Clone,
    Event: Send + 'static,
    Resources: Send + Sync + 'static,
{
    fn from_context(ctx: &'ctx EffectContext<Event, Resources>) -> Self {
        Resource(ctx.resource::<T, I>().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::EmptyStorage;

    #[derive(Debug, Clone, Default)]
    struct TestModel {
        counter: i32,
        name: String,
    }

    #[derive(Debug, Clone)]
    struct TestResource {
        url: String,
    }

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log(String),
    }

    // Example extractor that clones the counter value
    struct CounterValue(i32);

    // Example resource extractor
    struct UrlValue(String);

    #[test]
    fn test_event_one_ref_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 42,
            name: "test".to_string(),
        });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let test_model = ModelRef::<TestModel>::from_context(&ctx);
        assert_eq!(test_model.counter, 42);
    }

    #[test]
    fn test_event_two_ref_extraction() {
        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 99,
                name: "tuple".to_string(),
            })
            .with_model(FirstModelType { value: 42 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Extract two different model types from context
        let test_model = ModelRef::<TestModel>::from_context(&ctx);
        let first_model = ModelRef::<FirstModelType>::from_context(&ctx);

        assert_eq!(test_model.counter, 99);
        assert_eq!(first_model.value, 42);
    }

    #[test]
    fn test_event_one_mut_extraction() {
        let test_model = TestModel {
            counter: 123,
            name: "model_ref_test".to_string(),
        };
        let mut storage = EmptyStorage.with_model(test_model.clone());
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let mut model_mut = ModelMut::<TestModel>::from_context(&ctx);
        assert_eq!(model_mut.counter, 123);
        assert_eq!(model_mut.name, "model_ref_test");

        // Mutate the model
        model_mut.counter = 456;
        model_mut.name = "mutated_name".to_string();

        // Verify the mutations
        assert_eq!(model_mut.counter, 456);
        assert_eq!(model_mut.name, "mutated_name");
    }

    #[test]
    fn test_event_two_mut_extraction() {
        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 99,
                name: "tuple".to_string(),
            })
            .with_model(FirstModelType { value: 42 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Extract two different model types mutably from context
        let mut test_model = ModelMut::<TestModel>::from_context(&ctx);
        let mut first_model = ModelMut::<FirstModelType>::from_context(&ctx);

        // Verify initial values
        assert_eq!(test_model.counter, 99);
        assert_eq!(first_model.value, 42);

        // Mutate both models
        test_model.counter = 200;
        test_model.name = "mutated".to_string();
        first_model.value = 84;

        // Verify mutations
        assert_eq!(test_model.counter, 200);
        assert_eq!(test_model.name, "mutated");
        assert_eq!(first_model.value, 84);
    }

    #[test]
    fn test_event_ref_and_mut_extraction() {
        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 100,
                name: "original".to_string(),
            })
            .with_model(FirstModelType { value: 50 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Extract one model as ref and another as mut
        let test_model_ref = ModelRef::<TestModel>::from_context(&ctx);
        let mut first_model_mut = ModelMut::<FirstModelType>::from_context(&ctx);

        // Verify initial values
        assert_eq!(test_model_ref.counter, 100);
        assert_eq!(test_model_ref.name, "original");
        assert_eq!(first_model_mut.value, 50);

        // Mutate only the mutable model
        first_model_mut.value = 150;

        // Verify the ref model is unchanged and mut model is changed
        assert_eq!(test_model_ref.counter, 100);
        assert_eq!(test_model_ref.name, "original");
        assert_eq!(first_model_mut.value, 150);
    }

    #[derive(Debug, Clone, Default)]
    struct FirstModelType {
        value: i32,
    }
}
