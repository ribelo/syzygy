//! Context-based extractors for magic handlers
//!
//! This module provides traits for extracting values from EventContext and EffectContext
//! to enable Axum-style magic parameter injection in handler functions.
//!
//! The extractors work with the current Syzygy EventContext and EffectContext designs,
//! providing type-safe access to models and resources through the Storage system.

use crate::event_context::EventContext;
use crate::async_context::EffectContext;
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
pub trait FromEventContext<Event, Effect, Storage> {
    /// Extract Self from an EventContext
    fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self;
}

/// Extract a value from an EventContext (mutable access)
///
/// This trait enables extraction that requires mutable access to the EventContext.
/// The extraction happens inside the magic handler's call method where mutable
/// access is available.
///
/// # Example
/// ```rust,ignore
/// struct UserCountMut<'a>(&'a mut i32);
///
/// impl<Event, Effect, Storage> FromEventContextMut<Event, Effect, Storage> for UserCountMut<'_>
/// where
///     Storage: Selector<UserModel, syzygy::storage::storage::Here>,
/// {
///     fn from_context_mut(ctx: &mut EventContext<Event, Effect, Storage>) -> Self {
///         UserCountMut(&mut ctx.model_mut::<UserModel, syzygy::storage::storage::Here>().count)
///     }
/// }
/// ```
pub trait FromEventContextMut<Event, Effect, Storage> {
    /// Extract Self from a mutable EventContext
    fn from_context_mut(ctx: &mut EventContext<Event, Effect, Storage>) -> Self;
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
pub trait FromEffectContext<Event, Resources> {
    /// Extract Self from an EffectContext
    fn from_context(ctx: &EffectContext<Event, Resources>) -> Self;
}

// ============================================================================
// Common Extractor Implementations
// ============================================================================

/// Wrapper for extracting a cloned model from EventContext
///
/// This provides an ergonomic way to extract an entire model as an owned value.
/// Useful when the handler needs to work with the full model data.
///
/// # Example
/// ```rust,ignore
/// fn handle_event(event: MyEvent, model: ModelRef<UserModel>) -> Command<Event, Effect> {
///     println!("Current user: {}", model.0.name);
///     Command::none()
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ModelRef<T>(pub T);

/// Wrapper for extracting a resource reference from EffectContext
///
/// This provides access to resources in the EffectContext.
/// Since resources are stored in Arc, we can provide references.
///
/// # Example
/// ```rust,ignore
/// fn handle_effect(effect: MyEffect, db: ResourceRef<DatabaseResource>) -> impl Future<Output = ()> {
///     async move {
///         println!("Using database: {}", db.0.url);
///     }
/// }
/// ```
#[derive(Debug)]
pub struct ResourceRef<'a, T>(pub &'a T);

// Default implementations for ModelRef<T> - head position only for now
impl<Event, Effect, Storage, T> FromEventContext<Event, Effect, Storage> for ModelRef<T>
where
    Storage: Selector<T, crate::storage::storage::Here>,
    T: Clone,
{
    fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
        ModelRef(ctx.model::<T, crate::storage::storage::Here>().clone())
    }
}

// Note: ResourceRef implementation commented out due to lifetime complexity.
// Users should access resources directly in their effect handlers for now.
// TODO: Implement ResourceRef properly or find alternative approach

// ============================================================================
// Tuple Implementations for Multiple Extractions
// ============================================================================

impl<Event, Effect, Storage> FromEventContext<Event, Effect, Storage> for () {
    fn from_context(_ctx: &EventContext<Event, Effect, Storage>) -> Self {
        ()
    }
}

impl<Event, Effect, Storage, A> FromEventContext<Event, Effect, Storage> for (A,)
where
    A: FromEventContext<Event, Effect, Storage>,
{
    fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
        (A::from_context(ctx),)
    }
}

impl<Event, Effect, Storage, A, B> FromEventContext<Event, Effect, Storage> for (A, B)
where
    A: FromEventContext<Event, Effect, Storage>,
    B: FromEventContext<Event, Effect, Storage>,
{
    fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
        (A::from_context(ctx), B::from_context(ctx))
    }
}

impl<Event, Effect, Storage, A, B, C> FromEventContext<Event, Effect, Storage> for (A, B, C)
where
    A: FromEventContext<Event, Effect, Storage>,
    B: FromEventContext<Event, Effect, Storage>,
    C: FromEventContext<Event, Effect, Storage>,
{
    fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
        (
            A::from_context(ctx),
            B::from_context(ctx),
            C::from_context(ctx),
        )
    }
}

impl<Event, Effect, Storage, A, B, C, D> FromEventContext<Event, Effect, Storage> for (A, B, C, D)
where
    A: FromEventContext<Event, Effect, Storage>,
    B: FromEventContext<Event, Effect, Storage>,
    C: FromEventContext<Event, Effect, Storage>,
    D: FromEventContext<Event, Effect, Storage>,
{
    fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
        (
            A::from_context(ctx),
            B::from_context(ctx),
            C::from_context(ctx),
            D::from_context(ctx),
        )
    }
}

// Mutable tuple implementations - limited due to Rust borrow rules
impl<Event, Effect, Storage> FromEventContextMut<Event, Effect, Storage> for () {
    fn from_context_mut(_ctx: &mut EventContext<Event, Effect, Storage>) -> Self {
        ()
    }
}

impl<Event, Effect, Storage, A> FromEventContextMut<Event, Effect, Storage> for (A,)
where
    A: FromEventContextMut<Event, Effect, Storage>,
{
    fn from_context_mut(ctx: &mut EventContext<Event, Effect, Storage>) -> Self {
        (A::from_context_mut(ctx),)
    }
}

// Note: Multiple mutable extractions are complex due to borrow checker.
// Users should use sequential extraction in their handlers instead.

// EffectContext tuple implementations
impl<Event, Resources> FromEffectContext<Event, Resources> for () {
    fn from_context(_ctx: &EffectContext<Event, Resources>) -> Self {
        ()
    }
}

impl<Event, Resources, A> FromEffectContext<Event, Resources> for (A,)
where
    A: FromEffectContext<Event, Resources>,
{
    fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
        (A::from_context(ctx),)
    }
}

impl<Event, Resources, A, B> FromEffectContext<Event, Resources> for (A, B)
where
    A: FromEffectContext<Event, Resources>,
    B: FromEffectContext<Event, Resources>,
{
    fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
        (A::from_context(ctx), B::from_context(ctx))
    }
}

impl<Event, Resources, A, B, C> FromEffectContext<Event, Resources> for (A, B, C)
where
    A: FromEffectContext<Event, Resources>,
    B: FromEffectContext<Event, Resources>,
    C: FromEffectContext<Event, Resources>,
{
    fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
        (
            A::from_context(ctx),
            B::from_context(ctx),
            C::from_context(ctx),
        )
    }
}

impl<Event, Resources, A, B, C, D> FromEffectContext<Event, Resources> for (A, B, C, D)
where
    A: FromEffectContext<Event, Resources>,
    B: FromEffectContext<Event, Resources>,
    C: FromEffectContext<Event, Resources>,
    D: FromEffectContext<Event, Resources>,
{
    fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
        (
            A::from_context(ctx),
            B::from_context(ctx),
            C::from_context(ctx),
            D::from_context(ctx),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Storage, EmptyStorage};

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

    impl<Event, Effect, Storage> FromEventContext<Event, Effect, Storage> for CounterValue
    where
        Storage: Selector<TestModel, crate::storage::storage::Here>,
    {
        fn from_context(ctx: &EventContext<Event, Effect, Storage>) -> Self {
            CounterValue(ctx.model::<TestModel, crate::storage::storage::Here>().counter)
        }
    }

    // Example resource extractor
    struct UrlValue(String);

    impl<Event, Resources> FromEffectContext<Event, Resources> for UrlValue
    where
        Resources: Selector<TestResource, crate::storage::storage::Here>,
        Event: Send + 'static,
        Resources: Send + Sync + 'static,
    {
        fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
            UrlValue(ctx.resource::<TestResource, crate::storage::storage::Here>().url.clone())
        }
    }

    #[test]
    fn test_event_context_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 42,
            name: "test".to_string(),
        });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let counter_value = CounterValue::from_context(&ctx);
        assert_eq!(counter_value.0, 42);
    }

    #[test]
    fn test_tuple_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 99,
            name: "tuple".to_string(),
        });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let (counter1, counter2): (CounterValue, CounterValue) = FromEventContext::from_context(&ctx);
        assert_eq!(counter1.0, 99);
        assert_eq!(counter2.0, 99);
    }

    #[test]
    fn test_unit_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel::default());
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let _unit: () = FromEventContext::from_context(&ctx);
        // Test passes if it compiles and runs
    }

    // Note: ModelRef implementation for TestModel is now provided by default generic implementation

    #[test]
    fn test_model_ref_extraction() {
        let test_model = TestModel {
            counter: 123,
            name: "model_ref_test".to_string(),
        };
        let mut storage = EmptyStorage.with_model(test_model.clone());
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let model_ref = ModelRef::<TestModel>::from_context(&ctx);
        assert_eq!(model_ref.0.counter, 123);
        assert_eq!(model_ref.0.name, "model_ref_test");
    }

    #[test]
    fn test_resource_ref_extraction() {
        use crate::async_context::EffectContext;
        use crate::storage::EmptyStorage;
        use std::sync::Arc;

        let resource = TestResource {
            url: "https://example.com".to_string(),
        };
        let resources = Arc::new(EmptyStorage.with_model(resource));

        let ctx = EffectContext::<TestEvent, _>::new(None, resources);
        let url_value = UrlValue::from_context(&ctx);

        assert_eq!(url_value.0, "https://example.com");
    }
}
