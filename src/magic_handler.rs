//! Magic handler system for automatic parameter extraction
//!
//! This module implements Axum-style magic handlers that automatically extract
//! parameters from EventContext and EffectContext. Magic handlers enable clean,
//! focused handler functions that only declare the data they need.

use crate::async_context::EffectContext;
use crate::command::Command;
use crate::event_context::EventContext;
use crate::extract::{FromEffectContext, FromEventContext};
use std::future::Future;

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
pub trait EventMagicHandler<'a, Event, Effect, Storage, Args> {
    /// Call the handler with automatic parameter extraction from EventContext
    fn call(
        self,
        event: Event,
        ctx: &'a EventContext<Event, Effect, Storage>,
    ) -> Command<Event, Effect>;
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
    /// Call the handler with automatic parameter extraction from EffectContext
    fn call(self, effect: Effect, ctx: EffectContext<Event, Resources>)
    -> impl Future<Output = ()>;
}

// ============================================================================
// EventMagicHandler Implementations
// ============================================================================

// Handler with only event parameter (no extraction)
impl<'a, Event, Effect, Storage, F> EventMagicHandler<'a, Event, Effect, Storage, ()> for F
where
    F: FnOnce(Event) -> Command<Event, Effect>,
{
    fn call(
        self,
        event: Event,
        _ctx: &'a EventContext<Event, Effect, Storage>,
    ) -> Command<Event, Effect> {
        self(event)
    }
}

// Handler with event + one extracted parameter
impl<'a, Event, Effect, Storage, F, T, I> EventMagicHandler<'a, Event, Effect, Storage, (T, I)> for F
where
    F: FnOnce(Event, T) -> Command<Event, Effect>,
    T: FromEventContext<'a, Event, Effect, Storage, I>,
{
    fn call(
        self,
        event: Event,
        ctx: &'a EventContext<Event, Effect, Storage>,
    ) -> Command<Event, Effect> {
        let t = T::from_context(ctx);
        self(event, t)
    }
}

// Handler with event + two extracted parameters
impl<'a, Event, Effect, Storage, F, T1, I1, T2, I2>
    EventMagicHandler<'a, Event, Effect, Storage, (T1, I1, T2, I2)> for F
where
    F: FnOnce(Event, T1, T2) -> Command<Event, Effect>,
    T1: FromEventContext<'a, Event, Effect, Storage, I1>,
    T2: FromEventContext<'a, Event, Effect, Storage, I2>,
{
    fn call(
        self,
        event: Event,
        ctx: &'a EventContext<Event, Effect, Storage>,
    ) -> Command<Event, Effect> {
        let t1 = T1::from_context(ctx);
        let t2 = T2::from_context(ctx);
        self(event, t1, t2)
    }
}

// Handler with event + three extracted parameters
impl<'a, Event, Effect, Storage, F, T1, I1, T2, I2, T3, I3>
    EventMagicHandler<'a, Event, Effect, Storage, (T1, I1, T2, I2, T3, I3)> for F
where
    F: FnOnce(Event, T1, T2, T3) -> Command<Event, Effect>,
    T1: FromEventContext<'a, Event, Effect, Storage, I1>,
    T2: FromEventContext<'a, Event, Effect, Storage, I2>,
    T3: FromEventContext<'a, Event, Effect, Storage, I3>,
{
    fn call(
        self,
        event: Event,
        ctx: &'a EventContext<Event, Effect, Storage>,
    ) -> Command<Event, Effect> {
        let t1 = T1::from_context(ctx);
        let t2 = T2::from_context(ctx);
        let t3 = T3::from_context(ctx);
        self(event, t1, t2, t3)
    }
}

// ============================================================================
// EffectMagicHandler Implementations
// ============================================================================

// Handler with only effect parameter (no extraction)
impl<Effect, Event, Resources, F, Fut> EffectMagicHandler<Effect, Event, Resources, ()> for F
where
    F: FnOnce(Effect) -> Fut,
    Fut: Future<Output = ()>,
{
    fn call(
        self,
        effect: Effect,
        _ctx: EffectContext<Event, Resources>,
    ) -> impl Future<Output = ()> {
        self(effect)
    }
}

// Handler with effect + one extracted parameter
impl<Effect, Event, Resources, F, T1, I1, Fut>
    EffectMagicHandler<Effect, Event, Resources, (T1, I1)> for F
where
    F: FnOnce(Effect, T1) -> Fut,
    T1: for<'a> FromEffectContext<'a, Event, Resources, I1>,
    Fut: Future<Output = ()>,
{
    fn call(
        self,
        effect: Effect,
        ctx: EffectContext<Event, Resources>,
    ) -> impl Future<Output = ()> {
        let t1 = T1::from_context(&ctx);
        self(effect, t1)
    }
}

// Handler with effect + two extracted parameters
impl<Effect, Event, Resources, F, T1, I1, T2, I2, Fut>
    EffectMagicHandler<Effect, Event, Resources, (T1, I1, T2, I2)> for F
where
    F: FnOnce(Effect, T1, T2) -> Fut,
    T1: for<'a> FromEffectContext<'a, Event, Resources, I1>,
    T2: for<'a> FromEffectContext<'a, Event, Resources, I2>,
    Fut: Future<Output = ()>,
{
    fn call(
        self,
        effect: Effect,
        ctx: EffectContext<Event, Resources>,
    ) -> impl Future<Output = ()> {
        let t1 = T1::from_context(&ctx);
        let t2 = T2::from_context(&ctx);
        self(effect, t1, t2)
    }
}

// Handler with effect + three extracted parameters
impl<Effect, Event, Resources, F, T1, I1, T2, I2, T3, I3, Fut>
    EffectMagicHandler<Effect, Event, Resources, (T1, I1, T2, I2, T3, I3)> for F
where
    F: FnOnce(Effect, T1, T2, T3) -> Fut,
    T1: for<'a> FromEffectContext<'a, Event, Resources, I1>,
    T2: for<'a> FromEffectContext<'a, Event, Resources, I2>,
    T3: for<'a> FromEffectContext<'a, Event, Resources, I3>,
    Fut: Future<Output = ()>,
{
    fn call(
        self,
        effect: Effect,
        ctx: EffectContext<Event, Resources>,
    ) -> impl Future<Output = ()> {
        let t1 = T1::from_context(&ctx);
        let t2 = T2::from_context(&ctx);
        let t3 = T3::from_context(&ctx);
        self(effect, t1, t2, t3)
    }
}

pub fn event_trigger<'a, Event, Effect, Storage, Args, H>(
    event: Event,
    context: &'a EventContext<Event, Effect, Storage>,
    handler: H,
) -> Command<Event, Effect>
where
    H: EventMagicHandler<'a, Event, Effect, Storage, Args>,
{
    handler.call(event, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{FromEventContext, ModelMut};
    use crate::prelude::ModelRef;
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

    #[test]
    fn test_event_only_handler() {
        let mut storage = EmptyStorage.with_model(TestModel::default());
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

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
        let _result = event_trigger(event, &ctx, simple_handler);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_event_with_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 42,
            name: "test".to_string(),
        });
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        fn handler_with_extraction(
            event: TestEvent,
            current: ModelMut<'_, TestModel>,
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment { by } => {
                    println!("Current: {}, incrementing by {}", current.counter, by);
                    Command::none()
                }
                _ => Command::none(),
            }
        }
        // THIS WOKRS
        let _test_model = ModelMut::<TestModel>::from_context(&ctx);

        let event = TestEvent::Increment { by: 3 };
        // SO THIS ALSO SHOULD WORK
        let _result = event_trigger(event, &ctx, handler_with_extraction);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_multiple_extractions() {
        #[derive(Debug, Clone, Default)]
        struct SecondModel {
            value: i32,
        }

        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 99,
                name: "multi".to_string(),
            })
            .with_model(SecondModel { value: 42 });
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        fn handler_multi_extraction(
            event: TestEvent,
            model1: ModelRef<'_, TestModel>,
            model2: ModelRef<'_, SecondModel>,
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment { by } => {
                    println!(
                        "Model1 counter: {}, Model2 value: {}, incrementing by {}",
                        model1.counter, model2.value, by
                    );
                    Command::none()
                }
                _ => Command::none(),
            }
        }

        let event = TestEvent::Increment { by: 1 };
        let _result = event_trigger(event, &ctx, handler_multi_extraction);
        // Test passes if it compiles and runs
    }
}
