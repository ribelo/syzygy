//! Magic handler system for automatic parameter extraction
//!
//! This module implements Axum-style magic handlers that automatically extract
//! parameters from EventContext and EffectContext. Magic handlers enable clean,
//! focused handler functions that only declare the data they need.

use std::future::Future;
use crate::extract::{FromEventContext, FromEffectContext};
use crate::event_context::EventContext;
use crate::async_context::EffectContext;
use crate::command::Command;

/// Magic handler trait for update functions with automatic parameter extraction
///
/// This trait enables handler functions to declare exactly which data they need
/// from the EventContext, and the system automatically extracts and provides it.
/// This creates clean, focused handlers that are easy to test and compose.
///
/// # Type Parameters
///
/// - `Event`: The event type being handled
/// - `Effect`: The effect type that can be produced
/// - `Storage`: The storage type containing models
/// - `Args`: The argument tuple type for extraction
///
/// # Examples
///
/// ```rust,ignore
/// use syzygy::magic_handler::EventMagicHandler;
/// use syzygy::extract::FromEventContext;
///
/// // Define an extractor for a specific field
/// struct UserCount(i32);
/// impl<E, Ef, S> FromEventContext<E, Ef, S> for UserCount
/// where S: Selector<UserModel, Here> {
///     fn from_context(ctx: &EventContext<E, Ef, S>) -> Self {
///         UserCount(ctx.model::<UserModel, Here>().count)
///     }
/// }
///
/// // Handler function with magic parameter extraction
/// fn handle_user_created(
///     event: UserCreatedEvent,
///     count: UserCount,
/// ) -> Command<Event, Effect> {
///     println!("User created! Total users: {}", count.0);
///     Command::none()
/// }
///
/// // Usage
/// let result = handle_user_created.call_with_event(event, &mut ctx);
/// ```
pub trait EventMagicHandler<Event, Effect, Storage, Args> {
    type Output;

    /// Call the handler with automatic parameter extraction from EventContext
    fn call_with_event(self, event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output;
}

/// Magic handler trait for effect handlers with automatic parameter extraction
///
/// This trait enables async effect handler functions to declare exactly which
/// resources they need from the EffectContext, with automatic extraction.
///
/// # Type Parameters
///
/// - `Effect`: The effect type being handled
/// - `Event`: The event type that can be sent back
/// - `Resources`: The resources type containing dependencies
/// - `Args`: The argument tuple type for extraction
///
/// # Examples
///
/// ```rust,ignore
/// use syzygy::magic_handler::EffectMagicHandler;
/// use syzygy::extract::FromEffectContext;
///
/// // Define a resource extractor
/// struct DatabaseUrl(String);
/// impl<E, R> FromEffectContext<E, R> for DatabaseUrl
/// where R: Selector<DatabaseResource, Here> {
///     fn from_context(ctx: &EffectContext<E, R>) -> Self {
///         DatabaseUrl(ctx.resource::<DatabaseResource, Here>().url.clone())
///     }
/// }
///
/// // Async handler with magic parameter extraction
/// async fn handle_save_user(
///     effect: SaveUserEffect,
///     db_url: DatabaseUrl,
/// ) {
///     println!("Saving user to database at {}", db_url.0);
/// }
///
/// // Usage
/// handle_save_user.call_with_effect(effect, ctx).await;
/// ```
pub trait EffectMagicHandler<Effect, Event, Resources, Args> {
    type Output: Future<Output = ()>;

    /// Call the handler with automatic parameter extraction from EffectContext
    fn call_with_effect(self, effect: Effect, ctx: EffectContext<Event, Resources>) -> Self::Output;
}

// ============================================================================
// EventMagicHandler Implementations
// ============================================================================

// Handler with only event parameter (no extraction)
impl<Event, Effect, Storage, F, R> EventMagicHandler<Event, Effect, Storage, (Event,)> for F
where
    F: FnOnce(Event) -> R,
{
    type Output = R;

    fn call_with_event(self, event: Event, _ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output {
        self(event)
    }
}

// Handler with event + one extracted parameter
impl<Event, Effect, Storage, F, T, R> EventMagicHandler<Event, Effect, Storage, (Event, T)> for F
where
    F: FnOnce(Event, T) -> R,
    T: for<'a> FromEventContext<'a, Event, Effect, Storage>,
{
    type Output = R;

    fn call_with_event(self, event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output {
        let a = T::from_context(ctx);
        self(event, a)
    }
}

// Handler with event + two extracted parameters
impl<Event, Effect, Storage, F, A, B, R> EventMagicHandler<Event, Effect, Storage, (Event, A, B)> for F
where
    F: FnOnce(Event, A, B) -> R,
    A: for<'a> FromEventContext<'a, Event, Effect, Storage>,
    B: for<'a> FromEventContext<'a, Event, Effect, Storage>,
{
    type Output = R;

    fn call_with_event(self, event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output {
        let a = A::from_context(ctx);
        let b = B::from_context(ctx);
        self(event, a, b)
    }
}

// Handler with event + three extracted parameters
impl<Event, Effect, Storage, F, A, B, C, R> EventMagicHandler<Event, Effect, Storage, (Event, A, B, C)> for F
where
    F: FnOnce(Event, A, B, C) -> R,
    A: for<'a> FromEventContext<'a, Event, Effect, Storage>,
    B: for<'a> FromEventContext<'a, Event, Effect, Storage>,
    C: for<'a> FromEventContext<'a, Event, Effect, Storage>,
{
    type Output = R;

    fn call_with_event(self, event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output {
        let a = A::from_context(ctx);
        let b = B::from_context(ctx);
        let c = C::from_context(ctx);
        self(event, a, b, c)
    }
}

// Handler with event + four extracted parameters
impl<Event, Effect, Storage, F, A, B, C, D, R> EventMagicHandler<Event, Effect, Storage, (Event, A, B, C, D)> for F
where
    F: FnOnce(Event, A, B, C, D) -> R,
    A: for<'a> FromEventContext<'a, Event, Effect, Storage>,
    B: for<'a> FromEventContext<'a, Event, Effect, Storage>,
    C: for<'a> FromEventContext<'a, Event, Effect, Storage>,
    D: for<'a> FromEventContext<'a, Event, Effect, Storage>,
{
    type Output = R;

    fn call_with_event(self, event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output {
        let a = A::from_context(ctx);
        let b = B::from_context(ctx);
        let c = C::from_context(ctx);
        let d = D::from_context(ctx);
        self(event, a, b, c, d)
    }
}

// ============================================================================
// EffectMagicHandler Implementations
// ============================================================================

// Handler with only effect parameter (no extraction)
impl<Effect, Event, Resources, F, Fut> EffectMagicHandler<Effect, Event, Resources, (Effect,)> for F
where
    F: FnOnce(Effect) -> Fut,
    Fut: Future<Output = ()>,
{
    type Output = Fut;

    fn call_with_effect(self, effect: Effect, _ctx: EffectContext<Event, Resources>) -> Self::Output {
        self(effect)
    }
}

// Handler with effect + one extracted parameter
impl<Effect, Event, Resources, F, A, Fut> EffectMagicHandler<Effect, Event, Resources, (Effect, A)> for F
where
    F: FnOnce(Effect, A) -> Fut,
    A: FromEffectContext<Event, Resources>,
    Fut: Future<Output = ()>,
{
    type Output = Fut;

    fn call_with_effect(self, effect: Effect, ctx: EffectContext<Event, Resources>) -> Self::Output {
        let a = A::from_context(&ctx);
        self(effect, a)
    }
}

// Handler with effect + two extracted parameters
impl<Effect, Event, Resources, F, A, B, Fut> EffectMagicHandler<Effect, Event, Resources, (Effect, A, B)> for F
where
    F: FnOnce(Effect, A, B) -> Fut,
    A: FromEffectContext<Event, Resources>,
    B: FromEffectContext<Event, Resources>,
    Fut: Future<Output = ()>,
{
    type Output = Fut;

    fn call_with_effect(self, effect: Effect, ctx: EffectContext<Event, Resources>) -> Self::Output {
        let a = A::from_context(&ctx);
        let b = B::from_context(&ctx);
        self(effect, a, b)
    }
}

// Handler with effect + three extracted parameters
impl<Effect, Event, Resources, F, A, B, C, Fut> EffectMagicHandler<Effect, Event, Resources, (Effect, A, B, C)> for F
where
    F: FnOnce(Effect, A, B, C) -> Fut,
    A: FromEffectContext<Event, Resources>,
    B: FromEffectContext<Event, Resources>,
    C: FromEffectContext<Event, Resources>,
    Fut: Future<Output = ()>,
{
    type Output = Fut;

    fn call_with_effect(self, effect: Effect, ctx: EffectContext<Event, Resources>) -> Self::Output {
        let a = A::from_context(&ctx);
        let b = B::from_context(&ctx);
        let c = C::from_context(&ctx);
        self(effect, a, b, c)
    }
}

// Handler with effect + four extracted parameters
impl<Effect, Event, Resources, F, A, B, C, D, Fut> EffectMagicHandler<Effect, Event, Resources, (Effect, A, B, C, D)> for F
where
    F: FnOnce(Effect, A, B, C, D) -> Fut,
    A: FromEffectContext<Event, Resources>,
    B: FromEffectContext<Event, Resources>,
    C: FromEffectContext<Event, Resources>,
    D: FromEffectContext<Event, Resources>,
    Fut: Future<Output = ()>,
{
    type Output = Fut;

    fn call_with_effect(self, effect: Effect, ctx: EffectContext<Event, Resources>) -> Self::Output {
        let a = A::from_context(&ctx);
        let b = B::from_context(&ctx);
        let c = C::from_context(&ctx);
        let d = D::from_context(&ctx);
        self(effect, a, b, c, d)
    }
}

// ============================================================================
// Convenience Extensions
// ============================================================================

/// Extension trait to provide ergonomic `.call_magic()` syntax
pub trait EventMagicHandlerExt<Event, Effect, Storage, Args>: EventMagicHandler<Event, Effect, Storage, Args> {
    /// Call the magic handler (convenience method)
    fn call_magic(self, event: Event, ctx: &mut EventContext<Event, Effect, Storage>) -> Self::Output
    where
        Self: Sized,
    {
        self.call_with_event(event, ctx)
    }
}

/// Extension trait for effect magic handlers
pub trait EffectMagicHandlerExt<Effect, Event, Resources, Args>: EffectMagicHandler<Effect, Event, Resources, Args> {
    /// Call the effect magic handler (convenience method)
    fn call_magic(self, effect: Effect, ctx: EffectContext<Event, Resources>) -> Self::Output
    where
        Self: Sized,
    {
        self.call_with_effect(effect, ctx)
    }
}

// Blanket implementations for all magic handlers
impl<Event, Effect, Storage, Args, T> EventMagicHandlerExt<Event, Effect, Storage, Args> for T
where
    T: EventMagicHandler<Event, Effect, Storage, Args>,
{
}

impl<Effect, Event, Resources, Args, T> EffectMagicHandlerExt<Effect, Event, Resources, Args> for T
where
    T: EffectMagicHandler<Effect, Event, Resources, Args>,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::FromEventContext;
    use crate::storage::EmptyStorage;

    #[derive(Debug, Clone, Default)]
    struct TestModel {
        counter: i32,
        name: String,
    }

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment { by: i32 },
        SetName { name: String },
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log { message: String },
    }

    // Example extractor
    struct CounterValue(i32);

    impl<'a, Event, Effect, Storage> FromEventContext<'a, Event, Effect, Storage> for CounterValue
    where
        Storage: crate::storage::Selector<TestModel, crate::storage::storage::Here>,
    {
        fn from_context(ctx: &'a EventContext<Event, Effect, Storage>) -> Self {
            CounterValue(ctx.model::<TestModel, crate::storage::storage::Here>().counter)
        }
    }

    #[test]
    fn test_event_only_handler() {
        let mut storage = EmptyStorage.with_model(TestModel::default());
        let mut ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        fn simple_handler(event: TestEvent) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment { by } => {
                    println!("Incrementing by {}", by);
                    Command::none()
                }
                _ => Command::none(),
            }
        }

        let event = TestEvent::Increment { by: 5 };
        let _result = simple_handler.call_with_event(event, &mut ctx);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_event_with_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 42,
            name: "test".to_string(),
        });
        let mut ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        fn handler_with_extraction(
            event: TestEvent,
            current: CounterValue,
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment { by } => {
                    println!("Current: {}, incrementing by {}", current.0, by);
                    Command::none()
                }
                _ => Command::none(),
            }
        }

        let event = TestEvent::Increment { by: 3 };
        let _result = handler_with_extraction.call_with_event(event, &mut ctx);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_multiple_extractions() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 99,
            name: "multi".to_string(),
        });
        let mut ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        fn handler_multi_extraction(
            event: TestEvent,
            counter1: CounterValue,
            counter2: CounterValue,
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment { by } => {
                    println!("Counter1: {}, Counter2: {}, incrementing by {}", 
                            counter1.0, counter2.0, by);
                    assert_eq!(counter1.0, counter2.0); // Should be same value
                    Command::none()
                }
                _ => Command::none(),
            }
        }

        let event = TestEvent::Increment { by: 1 };
        let _result = handler_multi_extraction.call_with_event(event, &mut ctx);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_magic_handler_extension() {
        let mut storage = EmptyStorage.with_model(TestModel::default());
        let mut ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        fn test_handler(event: TestEvent) -> Command<TestEvent, TestEffect> {
            println!("Handling event: {:?}", event);
            Command::none()
        }

        let event = TestEvent::SetName { name: "test".to_string() };

        // Test both call methods work
        let _result1 = test_handler.call_with_event(event.clone(), &mut ctx);
        let _result2 = test_handler.call_magic(event, &mut ctx);
        // Test passes if it compiles and runs
    }
}
