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

/// Marker type for unit variant handlers
pub struct UnitHandler;

/// Handler trait for update functions with automatic parameter extraction
pub trait EventMagicHandler<'a, Variant, Event, Effect, Storage, Args> {
    /// Call the handler with automatic parameter extraction from EventContext
    fn call(
        self,
        event_variant: Variant,
        ctx: &'a EventContext<Event, Effect, Storage>,
    ) -> Command<Event, Effect>;
}

/// Handler trait for effect handlers with automatic parameter extraction  
pub trait EffectMagicHandler<Variant, Effect, Event, Resources, Args> {
    /// Call the handler with automatic parameter extraction from EffectContext
    fn call(
        self,
        effect_variant: Variant,
        ctx: EffectContext<Event, Resources>,
    ) -> impl Future<Output = ()>;
}

// ============================================================================
// Magic Handler Implementation Macros - Replaces 200+ lines of boilerplate
// ============================================================================

/// Generates EventMagicHandler implementations for 0-N parameters
macro_rules! impl_event_magic_handler {
    // Base case: no extractors, just event
    () => {
        // Handler with no parameters (unit variants)
        impl<'a, Event, Effect, Storage, F> EventMagicHandler<'a, (), Event, Effect, Storage, UnitHandler>
            for F
        where
            F: Fn() -> Command<Event, Effect>,
        {
            fn call(self, _: (), _ctx: &'a EventContext<Event, Effect, Storage>) -> Command<Event, Effect> {
                self()
            }
        }

        // Handler with only event parameter (no extraction)
        impl<'a, Variant, Event, Effect, Storage, F>
            EventMagicHandler<'a, Variant, Event, Effect, Storage, ()> for F
        where
            F: Fn(Variant) -> Command<Event, Effect>,
            Variant: TryFrom<Event>,
            <Variant as TryFrom<Event>>::Error: std::fmt::Debug,
        {
            fn call(
                self,
                event_variant: Variant,
                _ctx: &'a EventContext<Event, Effect, Storage>,
            ) -> Command<Event, Effect> {
                self(event_variant)
            }
        }
    };

    // Recursive case: generate impl for N parameters
    ($($T:ident, $I:ident),+) => {
        impl<'a, Variant, Event, Effect, Storage, F, $($T, $I),+>
            EventMagicHandler<'a, Variant, Event, Effect, Storage, ($($T, $I),+)> for F
        where
            F: Fn(Variant, $($T),+) -> Command<Event, Effect>,
            Variant: TryFrom<Event>,
            <Variant as TryFrom<Event>>::Error: std::fmt::Debug,
            $($T: FromEventContext<'a, Event, Effect, Storage, $I>),+
        {
            fn call(
                self,
                event_variant: Variant,
                ctx: &'a EventContext<Event, Effect, Storage>,
            ) -> Command<Event, Effect> {
                $(let $T = $T::from_context(ctx);)+
                self(event_variant, $($T),+)
            }
        }
    };
}

/// Generates EffectMagicHandler implementations for 0-N parameters  
macro_rules! impl_effect_magic_handler {
    // Base case: no extractors, just effect
    () => {
        // Handler with only effect parameter (no extraction)
        impl<Variant, Effect, Event, Resources, F, Fut>
            EffectMagicHandler<Variant, Effect, Event, Resources, ()> for F
        where
            F: Fn(Variant) -> Fut,
            Variant: TryFrom<Effect>,
            Fut: Future<Output = ()>,
        {
            fn call(
                self,
                effect_variant: Variant,
                _ctx: EffectContext<Event, Resources>,
            ) -> impl Future<Output = ()> {
                self(effect_variant)
            }
        }
    };

    // Recursive case: generate impl for N parameters
    ($($T:ident, $I:ident),+) => {
        impl<Variant, Effect, Event, Resources, F, $($T, $I),+ , Fut>
            EffectMagicHandler<Variant, Effect, Event, Resources, ($($T, $I),+)> for F
        where
            F: Fn(Variant, $($T),+) -> Fut,
            Variant: TryFrom<Effect>,
            $($T: for<'a> FromEffectContext<'a, Event, Resources, $I>),+,
            Fut: Future<Output = ()>,
        {
            fn call(
                self,
                effect_variant: Variant,
                ctx: EffectContext<Event, Resources>,
            ) -> impl Future<Output = ()> {
                $(let $T = $T::from_context(&ctx);)+
                self(effect_variant, $($T),+)
            }
        }
    };
}

// Generate implementations for 0-16 parameters (covers all practical use cases)
// Suppress non_snake_case warnings for generated macro parameter names
#[allow(non_snake_case)]
mod magic_handler_impls {
    use super::{Command, EventContext, EventMagicHandler, FromEventContext, UnitHandler};

    impl_event_magic_handler!();
    impl_event_magic_handler!(T1, I1);
    impl_event_magic_handler!(T1, I1, T2, I2);
    impl_event_magic_handler!(T1, I1, T2, I2, T3, I3);
    impl_event_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4);
    impl_event_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5);
    impl_event_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6);
    impl_event_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7);
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14, T15, I15
    );
    impl_event_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14, T15, I15, T16, I16
    );
}

#[allow(non_snake_case)]
mod effect_magic_handler_impls {
    use super::{EffectContext, EffectMagicHandler, FromEffectContext, Future};

    impl_effect_magic_handler!();
    impl_effect_magic_handler!(T1, I1);
    impl_effect_magic_handler!(T1, I1, T2, I2);
    impl_effect_magic_handler!(T1, I1, T2, I2, T3, I3);
    impl_effect_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4);
    impl_effect_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5);
    impl_effect_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6);
    impl_effect_magic_handler!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7);
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14, T15, I15
    );
    impl_effect_magic_handler!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14, T15, I15, T16, I16
    );
}

// ============================================================================
// Helper Functions - Keep existing API
// ============================================================================

pub fn event_trigger<'a, Variant, Event, Effect, Storage, Args, H>(
    event_variant: Variant,
    context: &'a EventContext<Event, Effect, Storage>,
    handler: H,
) -> Command<Event, Effect>
where
    H: EventMagicHandler<'a, Variant, Event, Effect, Storage, Args>,
    Variant: TryFrom<Event>,
{
    handler.call(event_variant, context)
}

pub fn effect_trigger<Variant, Effect, Event, Resources, Args, H>(
    effect_variant: Variant,
    context: EffectContext<Event, Resources>,
    handler: H,
) -> impl Future<Output = ()>
where
    H: EffectMagicHandler<Variant, Effect, Event, Resources, Args>,
    Variant: TryFrom<Effect>,
{
    handler.call(effect_variant, context)
}

// ============================================================================
// Direct Variant Magic Effect Handler (no TryFrom bound)
// ============================================================================

/// Magic handler trait for direct effect variant payloads
///
/// This mirrors EffectMagicHandler but drops the `Variant: TryFrom<Effect>`
/// bound so it can be used when you already have the concrete variant payload
/// (e.g., when routing to a specific executor).
pub trait EffectMagicHandlerDirect<Variant, Event, Resources, Args> {
    fn call(
        self,
        effect_variant: Variant,
        ctx: EffectContext<Event, Resources>,
    ) -> impl Future<Output = ()> + Send;
}

/// Generates EffectMagicHandlerDirect implementations for 0..=16 parameters
macro_rules! impl_effect_magic_handler_direct {
    () => {
        impl<Variant, Event, Resources, F, Fut> EffectMagicHandlerDirect<Variant, Event, Resources, ()>
            for F
        where
            F: Fn(Variant) -> Fut,
            Fut: Future<Output = ()> + Send + 'static,
        {
            fn call(
                self,
                effect_variant: Variant,
                _ctx: EffectContext<Event, Resources>,
            ) -> impl Future<Output = ()> + Send {
                self(effect_variant)
            }
        }
    };

    ($($T:ident, $I:ident),+) => {
        impl<Variant, Event, Resources, F, $($T, $I),+, Fut>
            EffectMagicHandlerDirect<Variant, Event, Resources, ($($T, $I),+)> for F
        where
            F: Fn(Variant, $($T),+) -> Fut,
            $($T: for<'a> FromEffectContext<'a, Event, Resources, $I>),+,
            Fut: Future<Output = ()> + Send + 'static,
        {
            fn call(
                self,
                effect_variant: Variant,
                ctx: EffectContext<Event, Resources>,
            ) -> impl Future<Output = ()> + Send {
                $(let $T = $T::from_context(&ctx);)+
                self(effect_variant, $($T),+)
            }
        }
    };
}

#[allow(non_snake_case)]
mod effect_magic_handler_direct_impls {
    use super::{EffectContext, EffectMagicHandlerDirect, FromEffectContext, Future};

    impl_effect_magic_handler_direct!();
    impl_effect_magic_handler_direct!(T1, I1);
    impl_effect_magic_handler_direct!(T1, I1, T2, I2);
    impl_effect_magic_handler_direct!(T1, I1, T2, I2, T3, I3);
    impl_effect_magic_handler_direct!(T1, I1, T2, I2, T3, I3, T4, I4);
    impl_effect_magic_handler_direct!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5);
    impl_effect_magic_handler_direct!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6);
    impl_effect_magic_handler_direct!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7);
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14, T15, I15
    );
    impl_effect_magic_handler_direct!(
        T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
        T12, I12, T13, I13, T14, I14, T15, I15, T16, I16
    );
}

/// Trigger a direct-variant magic effect handler
pub fn effect_trigger_direct<Variant, Event, Resources, Args, H>(
    effect_variant: Variant,
    context: EffectContext<Event, Resources>,
    handler: H,
) -> impl Future<Output = ()> + Send
where
    H: EffectMagicHandlerDirect<Variant, Event, Resources, Args>,
{
    handler.call(effect_variant, context)
}

// ============================================================================
// Dispatch Macros - Keep existing API
// ============================================================================

/// Magic handler dispatch macro for events
#[macro_export]
macro_rules! event_magic_handler {
    ($event:expr, $ctx:expr, $enum_name:ident { $($variant:ident => $handler:expr),* $(,)? }) => {
        {
            use $crate::magic_handler::event_trigger;
            match $event {
                $(
                    $enum_name::$variant(variant) => event_trigger(variant, $ctx, $handler),
                )*
            }
        }
    };
}

/// Magic effect handler dispatch macro
#[macro_export]
macro_rules! effect_magic_handler {
    ($effect:expr, $ctx:expr, $enum_name:ident { $($variant:ident => $handler:expr),* $(,)? }) => {
        {
            use $crate::magic_handler::effect_trigger;
            match $effect {
                $(
                    $enum_name::$variant(variant) => {
                        effect_trigger::<_, $enum_name, _, _, (), _>(variant, $ctx, $handler).await;
                    }
                )*
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{EmptyStorage, StorageBuilder};
    use syzygy_macros::MagicVariants;

    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct TestModel {
        counter: i32,
        name: String,
    }

    #[derive(Debug, Clone)]
    struct Increment {
        by: i32,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct SetName {
        name: String,
    }

    #[derive(Debug, Clone, MagicVariants)]
    #[allow(dead_code)]
    enum TestEvent {
        Increment(Increment),
        SetName(SetName),
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEffect {
        Log { message: String },
    }

    #[test]
    fn test_event_only_handler() {
        fn simple_handler(event: Increment) -> Command<TestEvent, TestEffect> {
            println!("Incrementing by {}", event.by);
            Command::none()
        }

        let mut storage = EmptyStorage::new().with_model(TestModel::default());
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let event = Increment { by: 5 };
        let _result = event_trigger(event, &ctx, simple_handler);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_event_with_extraction() {
        fn handler_with_extraction(
            event: Increment,
            current: &TestModel,
        ) -> Command<TestEvent, TestEffect> {
            println!("Current: {}, incrementing by {}", current.counter, event.by);
            Command::none()
        }

        let mut storage = EmptyStorage::new().with_model(TestModel {
            counter: 42,
            name: "test".to_string(),
        });
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
        let event = Increment { by: 3 };
        let _result = event_trigger(event, &ctx, handler_with_extraction);
        // Test passes if it compiles and runs
    }

    #[test]
    fn test_multiple_extractions() {
        #[derive(Debug, Clone, Default)]
        struct SecondModel {
            value: i32,
        }

        fn handler_multi_extraction(
            event: Increment,
            model1: &TestModel,
            model2: &mut SecondModel,
        ) -> Command<TestEvent, TestEffect> {
            // Mutate model2 to test &mut T extraction
            model2.value += event.by;
            println!(
                "Model1 counter: {}, Model2 value: {}, incrementing by {}",
                model1.counter, model2.value, event.by
            );
            Command::none()
        }

        let mut storage = EmptyStorage::new()
            .with_model(TestModel {
                counter: 99,
                name: "multi".to_string(),
            })
            .with_model(SecondModel { value: 42 });
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let event = Increment { by: 1 };
        let _result = event_trigger(event, &ctx, handler_multi_extraction);
        // Test passes if it compiles and runs
    }
}
