use std::cell::Cell;
#[cfg(feature = "shell")]
use std::future::Future;
#[cfg(feature = "shell")]
use std::marker::PhantomData;

#[cfg(feature = "shell")]
use futures::Stream;

use crate::command::Command;
#[cfg(feature = "shell")]
use crate::executor::Task;
#[cfg(feature = "shell")]
use crate::resource::{DynamicEnv, Resource, ResourceMap, Selector};
#[cfg(feature = "shell")]
use crate::subscription::Subscription;

const EXTRACTION_PROGRAMMER_ERROR_HINT: &str =
    "This is a programmer error; adjust handler extractors to avoid overlapping borrows";

// ── Effect side ─────────────────────────────────────────────────────

#[cfg(feature = "shell")]
pub struct EffectContext<'a, Env = DynamicEnv> {
    resources: &'a ResourceMap,
    _env: PhantomData<fn() -> Env>,
}

#[cfg(feature = "shell")]
impl<'a> EffectContext<'a, DynamicEnv> {
    #[must_use]
    pub fn new(resources: &'a ResourceMap) -> Self {
        Self {
            resources,
            _env: PhantomData,
        }
    }
}

#[cfg(feature = "shell")]
impl<'a, Env> EffectContext<'a, Env> {
    pub(crate) fn typed(resources: &'a ResourceMap) -> Self {
        Self {
            resources,
            _env: PhantomData,
        }
    }

    #[must_use]
    pub fn resources(&self) -> &ResourceMap {
        self.resources
    }

    /// Borrow a registered resource without cloning it.
    ///
    /// Signature-based resource extraction via [`FromEffectContext`] clones.
    /// Use this helper for explicit non-cloning access to expensive resources.
    #[must_use]
    pub fn resource_ref<T: 'static>(&self) -> Option<&T> {
        self.resources.get_ref::<T>()
    }
}

#[cfg(feature = "shell")]
pub trait FromEffectContext {
    /// Extracts a value from the [`EffectContext`].
    ///
    /// This operation clones the stored resource value from the [`ResourceMap`].
    /// Prefer storing expensive resources behind shared ownership pointers such
    /// as `Arc<T>` or `Rc<T>` via `.with_resource(...)` to avoid repeated deep
    /// clones on effect dispatch. Use [`EffectContext::resource_ref`] for
    /// explicit borrow-only access.
    #[track_caller]
    fn from_context(ctx: &EffectContext<'_>) -> Self;
}

#[cfg(feature = "shell")]
fn panic_missing_resource<T: Resource>(caller: &'static std::panic::Location<'static>) -> ! {
    panic!(
        "Effect resource `{}` is not registered. Register it with .with_resource() during builder setup. This is a programmer error (requested at {}:{})",
        std::any::type_name::<T>(),
        caller.file(),
        caller.line()
    )
}

#[cfg(feature = "shell")]
impl<T: Resource> FromEffectContext for T {
    #[track_caller]
    fn from_context(ctx: &EffectContext<'_>) -> Self {
        let caller = std::panic::Location::caller();
        match ctx.resources().get::<T>() {
            Some(resource) => resource,
            None => panic_missing_resource::<T>(caller),
        }
    }
}

#[cfg(feature = "shell")]
pub trait FromTypedEffectContext<Env, Index> {
    /// Extracts a compile-time-proven resource from the [`EffectContext`].
    #[track_caller]
    fn from_typed_context(ctx: &EffectContext<'_, Env>) -> Self;
}

#[cfg(feature = "shell")]
impl<T, Env, Index> FromTypedEffectContext<Env, Index> for T
where
    T: Resource,
    Env: Selector<T, Index>,
{
    #[track_caller]
    fn from_typed_context(ctx: &EffectContext<'_, Env>) -> Self {
        let caller = std::panic::Location::caller();
        match ctx.resources().get::<T>() {
            Some(resource) => resource,
            None => panic_missing_resource::<T>(caller),
        }
    }
}

#[cfg(feature = "shell")]
pub struct FutureEffect;
#[cfg(feature = "shell")]
pub struct StreamEffect;

#[doc(hidden)]
#[cfg(feature = "shell")]
pub struct NP;

#[doc(hidden)]
#[cfg(feature = "shell")]
pub struct TypedResource<T, Index>(PhantomData<fn() -> (T, Index)>);

#[cfg(feature = "shell")]
pub trait EffectHandler<E: 'static, X: 'static, P, Marker, Env = DynamicEnv>: 'static {
    fn handle(&self, payload: P, ctx: &EffectContext<'_, Env>) -> Task<E, X>;
}

#[cfg(feature = "shell")]
impl<E, X, P, F, Env> EffectHandler<E, X, P, (), Env> for F
where
    F: Fn(P) -> Task<E, X> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext<'_, Env>) -> Task<E, X> {
        (self)(payload)
    }
}

#[cfg(feature = "shell")]
impl<E, X, F, Env> EffectHandler<E, X, (), NP, Env> for F
where
    F: Fn() -> Task<E, X> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, _payload: (), _ctx: &EffectContext<'_, Env>) -> Task<E, X> {
        (self)()
    }
}

#[cfg(feature = "shell")]
impl<E, X, P, F, Fut, Env> EffectHandler<E, X, P, FutureEffect, Env> for F
where
    F: Fn(P) -> Fut + 'static,
    Fut: Future<Output = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext<'_, Env>) -> Task<E, X> {
        Task::future((self)(payload))
    }
}

#[cfg(feature = "shell")]
impl<E, X, F, Fut, Env> EffectHandler<E, X, (), (FutureEffect, NP), Env> for F
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, _payload: (), _ctx: &EffectContext<'_, Env>) -> Task<E, X> {
        Task::future((self)())
    }
}

#[cfg(feature = "shell")]
impl<E, X, P, F, S, Env> EffectHandler<E, X, P, StreamEffect, Env> for F
where
    F: Fn(P) -> S + 'static,
    S: Stream<Item = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext<'_, Env>) -> Task<E, X> {
        Task::stream((self)(payload))
    }
}

#[cfg(feature = "shell")]
impl<E, X, F, S, Env> EffectHandler<E, X, (), (StreamEffect, NP), Env> for F
where
    F: Fn() -> S + 'static,
    S: Stream<Item = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, _payload: (), _ctx: &EffectContext<'_, Env>) -> Task<E, X> {
        Task::stream((self)())
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_effect_handler_task {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, F, $($T),+> EffectHandler<E, X, P, ($($T,)+)> for F
        where
            F: Fn(P, $($T),+) -> Task<E, X> + 'static,
            $($T: FromEffectContext,)+
            E: 'static,
            X: 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<'_>) -> Task<E, X> {
                (self)(payload, $($T::from_context(ctx)),+)
            }
        }
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_effect_handler_future {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, F, Fut, $($T),+> EffectHandler<E, X, P, (FutureEffect, $($T,)+)> for F
        where
            F: Fn(P, $($T),+) -> Fut + 'static,
            Fut: Future<Output = Command<E, X>> + 'static,
            $($T: FromEffectContext,)+
            E: 'static,
            X: 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<'_>) -> Task<E, X> {
                Task::future((self)(payload, $($T::from_context(ctx)),+))
            }
        }
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_effect_handler_stream {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, F, S, $($T),+> EffectHandler<E, X, P, (StreamEffect, $($T,)+)> for F
        where
            F: Fn(P, $($T),+) -> S + 'static,
            S: Stream<Item = Command<E, X>> + 'static,
            $($T: FromEffectContext,)+
            E: 'static,
            X: 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<'_>) -> Task<E, X> {
                Task::stream((self)(payload, $($T::from_context(ctx)),+))
            }
        }
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_typed_effect_handler_task {
    ($($T:ident : $I:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, F, Env, $($T, $I),+> EffectHandler<E, X, P, ($(TypedResource<$T, $I>,)+), Env> for F
        where
            F: Fn(P, $($T),+) -> Task<E, X> + 'static,
            $($T: FromTypedEffectContext<Env, $I>,)+
            Env: 'static,
            E: 'static,
            X: 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<'_, Env>) -> Task<E, X> {
                (self)(payload, $($T::from_typed_context(ctx)),+)
            }
        }
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_typed_effect_handler_future {
    ($($T:ident : $I:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, F, Fut, Env, $($T, $I),+> EffectHandler<E, X, P, (FutureEffect, $(TypedResource<$T, $I>,)+), Env> for F
        where
            F: Fn(P, $($T),+) -> Fut + 'static,
            Fut: Future<Output = Command<E, X>> + 'static,
            $($T: FromTypedEffectContext<Env, $I>,)+
            Env: 'static,
            E: 'static,
            X: 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<'_, Env>) -> Task<E, X> {
                Task::future((self)(payload, $($T::from_typed_context(ctx)),+))
            }
        }
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_typed_effect_handler_stream {
    ($($T:ident : $I:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, F, S, Env, $($T, $I),+> EffectHandler<E, X, P, (StreamEffect, $(TypedResource<$T, $I>,)+), Env> for F
        where
            F: Fn(P, $($T),+) -> S + 'static,
            S: Stream<Item = Command<E, X>> + 'static,
            $($T: FromTypedEffectContext<Env, $I>,)+
            Env: 'static,
            E: 'static,
            X: 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<'_, Env>) -> Task<E, X> {
                Task::stream((self)(payload, $($T::from_typed_context(ctx)),+))
            }
        }
    }
}

#[cfg(feature = "shell")]
impl_effect_handler_task!(T1);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6, T7);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6, T7, T8);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
#[cfg(feature = "shell")]
impl_effect_handler_task!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(feature = "shell")]
impl_effect_handler_future!(T1);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6, T7);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6, T7, T8);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
#[cfg(feature = "shell")]
impl_effect_handler_future!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6, T7);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6, T7, T8);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
#[cfg(feature = "shell")]
impl_effect_handler_stream!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10, T11: I11);
#[cfg(feature = "shell")]
impl_typed_effect_handler_task!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10, T11: I11, T12: I12);

#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10, T11: I11);
#[cfg(feature = "shell")]
impl_typed_effect_handler_future!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10, T11: I11, T12: I12);

#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10, T11: I11);
#[cfg(feature = "shell")]
impl_typed_effect_handler_stream!(T1: I1, T2: I2, T3: I3, T4: I4, T5: I5, T6: I6, T7: I7, T8: I8, T9: I9, T10: I10, T11: I11, T12: I12);

// ── Event side ──────────────────────────────────────────────────────

pub struct EventContext<M> {
    ptr: *mut M,
    whole_model_mut: Cell<bool>,
    whole_model_immut: Cell<u32>,
    fields_mut: Cell<[u64; 4]>,
    fields_immut: Cell<[u64; 4]>,
}

impl<M> EventContext<M> {
    pub(crate) fn new(model: &mut M) -> Self {
        Self {
            ptr: model as *mut M,
            whole_model_mut: Cell::new(false),
            whole_model_immut: Cell::new(0),
            fields_mut: Cell::new([0; 4]),
            fields_immut: Cell::new([0; 4]),
        }
    }

    #[track_caller]
    pub fn track_field_borrow(&self, field_index: u32, field_name: &str) {
        assert!(
            !self.whole_model_mut.get(),
            "field '{field_name}' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );
        assert!(
            self.whole_model_immut.get() == 0,
            "field '{field_name}' overlaps already-borrowed immutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        let fields_mut = self.fields_mut.get();
        assert!(
            !check_field_bit(fields_mut, field_index, field_name),
            "field '{field_name}' (index {field_index}) already borrowed mutably in this handler; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        let fields_immut = self.fields_immut.get();
        assert!(
            !check_field_bit(fields_immut, field_index, field_name),
            "field '{field_name}' overlaps already-borrowed immutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        self.fields_mut
            .set(set_field_bit(fields_mut, field_index, field_name));
    }

    #[track_caller]
    pub fn track_field_immut(&self, field_index: u32, field_name: &str) {
        assert!(
            !self.whole_model_mut.get(),
            "field '{field_name}' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        let fields_mut = self.fields_mut.get();
        assert!(
            !check_field_bit(fields_mut, field_index, field_name),
            "field '{field_name}' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        let fields_immut = self.fields_immut.get();
        self.fields_immut
            .set(set_field_bit(fields_immut, field_index, field_name));
    }

    #[track_caller]
    fn track_whole_model_mut(&self) {
        assert!(
            !self.whole_model_mut.get(),
            "field 'model' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );
        assert!(
            self.whole_model_immut.get() == 0,
            "field 'model' overlaps already-borrowed immutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );
        assert!(
            !check_any_field_set(self.fields_mut.get()),
            "field 'model' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );
        assert!(
            !check_any_field_set(self.fields_immut.get()),
            "field 'model' overlaps already-borrowed immutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        self.whole_model_mut.set(true);
    }

    #[track_caller]
    fn track_whole_model_immut(&self) {
        assert!(
            !self.whole_model_mut.get(),
            "field 'model' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );
        assert!(
            !check_any_field_set(self.fields_mut.get()),
            "field 'model' overlaps already-borrowed mutable region; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
        );

        let next = self
            .whole_model_immut
            .get()
            .checked_add(1)
            .unwrap_or_else(|| {
                panic!(
                    "field 'model' exceeded immutable borrow tracking limit; {EXTRACTION_PROGRAMMER_ERROR_HINT}"
                )
            });
        self.whole_model_immut.set(next);
    }

    pub(crate) fn borrow_guard(&self) -> BorrowGuard<'_, M> {
        BorrowGuard {
            ctx: self,
            whole_model_mut: self.whole_model_mut.get(),
            whole_model_immut: self.whole_model_immut.get(),
            fields_mut: self.fields_mut.get(),
            fields_immut: self.fields_immut.get(),
        }
    }

    /// # Safety
    /// Caller must have called `track_field_borrow` for this field first, and the
    /// field offset must be correct for type `T` within `M`.
    pub unsafe fn field_ptr<T>(&self, offset: usize) -> *mut T {
        self.ptr.cast::<u8>().add(offset).cast::<T>()
    }

    pub fn model_ptr(&self) -> *mut M {
        self.ptr
    }

    #[track_caller]
    pub fn handle<E, X, H, Marker>(&self, handler: H) -> Command<E, X>
    where
        H: EventHandler<E, X, (), M, Marker>,
    {
        handler.handle((), self)
    }
}

pub struct SubscriptionContext<M> {
    ptr: *const M,
}

impl<M> SubscriptionContext<M> {
    pub(crate) fn new(model: &M) -> Self {
        Self {
            ptr: model as *const M,
        }
    }

    #[must_use]
    pub fn model_ptr(&self) -> *const M {
        self.ptr
    }

    #[cfg(feature = "shell")]
    #[track_caller]
    pub fn handle<E, X, H, Marker>(&self, handler: H) -> Subscription<E, X>
    where
        H: SubscriptionHandler<E, X, (), M, Marker>,
        E: 'static,
        X: 'static,
    {
        handler.handle((), self)
    }
}

fn set_field_bit(mut bits: [u64; 4], field_index: u32, field_name: &str) -> [u64; 4] {
    let (word, mask) = field_bit(field_index, field_name);
    bits[word] |= mask;
    bits
}

fn check_field_bit(bits: [u64; 4], field_index: u32, field_name: &str) -> bool {
    let (word, mask) = field_bit(field_index, field_name);
    bits[word] & mask != 0
}

fn check_any_field_set(bits: [u64; 4]) -> bool {
    (bits[0] | bits[1] | bits[2] | bits[3]) != 0
}

fn field_bit(field_index: u32, field_name: &str) -> (usize, u64) {
    assert!(
        field_index < 256,
        "field '{field_name}' index {field_index} exceeds borrow tracker capacity (256); {EXTRACTION_PROGRAMMER_ERROR_HINT}",
    );

    let word = (field_index / 64) as usize;
    let mask = 1u64 << (field_index % 64);
    (word, mask)
}

pub(crate) struct BorrowGuard<'a, M> {
    ctx: &'a EventContext<M>,
    whole_model_mut: bool,
    whole_model_immut: u32,
    fields_mut: [u64; 4],
    fields_immut: [u64; 4],
}

impl<M> Drop for BorrowGuard<'_, M> {
    fn drop(&mut self) {
        self.ctx.whole_model_mut.set(self.whole_model_mut);
        self.ctx.whole_model_immut.set(self.whole_model_immut);
        self.ctx.fields_mut.set(self.fields_mut);
        self.ctx.fields_immut.set(self.fields_immut);
    }
}

pub trait Part<M> {
    #[track_caller]
    fn extract(ctx: &EventContext<M>) -> &Self;
}

#[allow(clippy::mut_from_ref)]
pub trait PartMut<M> {
    #[track_caller]
    fn extract_mut(ctx: &EventContext<M>) -> &mut Self;
}

pub trait SubscriptionPart<M> {
    #[track_caller]
    fn extract(ctx: &SubscriptionContext<M>) -> &Self;
}

impl<M> Part<M> for M {
    #[track_caller]
    fn extract(ctx: &EventContext<M>) -> &Self {
        let ptr = ctx.model_ptr();
        ctx.track_whole_model_immut();
        // SAFETY: EventContext holds a valid pointer to the active model for the current dispatch.
        unsafe { &*ptr }
    }
}

impl<M> SubscriptionPart<M> for M {
    #[track_caller]
    fn extract(ctx: &SubscriptionContext<M>) -> &Self {
        let ptr = ctx.model_ptr();
        // SAFETY: SubscriptionContext holds a valid pointer to the current model
        // for the duration of subscription reconciliation.
        unsafe { &*ptr }
    }
}

#[allow(clippy::mut_from_ref)]
impl<M> PartMut<M> for M {
    #[track_caller]
    fn extract_mut(ctx: &EventContext<M>) -> &mut Self {
        let ptr = ctx.model_ptr();
        ctx.track_whole_model_mut();
        // SAFETY: EventContext stores a valid mutable pointer to the active model and
        // bitmask borrow tracking enforces exclusive mutable access during handler execution.
        unsafe { &mut *ptr }
    }
}

pub trait EventHandler<E, X, P, M, Marker>: 'static {
    #[track_caller]
    fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X>;
}

impl<E, X, P, M, F> EventHandler<E, X, P, M, ()> for F
where
    F: Fn(P) -> Command<E, X> + 'static,
{
    #[track_caller]
    fn handle(&self, payload: P, _ctx: &EventContext<M>) -> Command<E, X> {
        (self)(payload)
    }
}

impl<E, X, M, F> EventHandler<E, X, (), M, EventNP<()>> for F
where
    F: Fn() -> Command<E, X> + 'static,
{
    #[track_caller]
    fn handle(&self, _payload: (), _ctx: &EventContext<M>) -> Command<E, X> {
        (self)()
    }
}

#[doc(hidden)]
pub struct Owned<T>(::core::marker::PhantomData<fn() -> T>);

#[doc(hidden)]
pub struct Mut<T>(::core::marker::PhantomData<fn(&mut T)>);

#[doc(hidden)]
pub struct EventNP<Inner>(::core::marker::PhantomData<fn() -> Inner>);

#[cfg(feature = "shell")]
pub trait SubscriptionHandler<E: 'static, X: 'static, P, M, Marker>: 'static {
    #[track_caller]
    fn handle(&self, payload: P, ctx: &SubscriptionContext<M>) -> Subscription<E, X>;
}

#[cfg(feature = "shell")]
impl<E, X, P, M, F> SubscriptionHandler<E, X, P, M, ()> for F
where
    F: Fn(P) -> Subscription<E, X> + 'static,
    E: 'static,
    X: 'static,
{
    #[track_caller]
    fn handle(&self, payload: P, _ctx: &SubscriptionContext<M>) -> Subscription<E, X> {
        (self)(payload)
    }
}

#[cfg(feature = "shell")]
macro_rules! impl_subscription_handler {
    ($($T:ident),+ $(,)?) => {
        #[allow(non_snake_case)]
        impl<E, X, P, M, F, $($T),+> SubscriptionHandler<E, X, P, M, ($(Owned<$T>,)+)> for F
        where
            F: for<'a> Fn(P, $(&'a $T),+) -> Subscription<E, X> + 'static,
            $($T: SubscriptionPart<M>,)+
            E: 'static,
            X: 'static,
        {
            #[track_caller]
            fn handle(&self, payload: P, ctx: &SubscriptionContext<M>) -> Subscription<E, X> {
                (self)(payload, $(<$T as SubscriptionPart<M>>::extract(ctx)),+)
            }
        }

        #[allow(non_snake_case)]
        impl<E, X, M, F, $($T),+> SubscriptionHandler<E, X, (), M, EventNP<($(Owned<$T>,)+)>> for F
        where
            F: for<'a> Fn($(&'a $T),+) -> Subscription<E, X> + 'static,
            $($T: SubscriptionPart<M>,)+
            E: 'static,
            X: 'static,
        {
            #[track_caller]
            fn handle(&self, _payload: (), ctx: &SubscriptionContext<M>) -> Subscription<E, X> {
                (self)($(<$T as SubscriptionPart<M>>::extract(ctx)),+)
            }
        }
    };
}

macro_rules! impl_event_handler {
    ($($T:ident),+ $(,)?) => {
        #[allow(non_snake_case)]
        impl<E, X, P, M, F, $($T),+> EventHandler<E, X, P, M, ($(Owned<$T>,)+)> for F
        where
            F: for<'a> Fn(P, $(&'a $T),+) -> Command<E, X> + 'static,
            $($T: Part<M>,)+
        {
            #[track_caller]
            fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X> {
                let _borrow_guard = ctx.borrow_guard();
                (self)(payload, $(<$T as Part<M>>::extract(ctx)),+)
            }
        }

        #[allow(non_snake_case)]
        impl<E, X, M, F, $($T),+> EventHandler<E, X, (), M, EventNP<($(Owned<$T>,)+)>> for F
        where
            F: for<'a> Fn($(&'a $T),+) -> Command<E, X> + 'static,
            $($T: Part<M>,)+
        {
            #[track_caller]
            fn handle(&self, _payload: (), ctx: &EventContext<M>) -> Command<E, X> {
                let _borrow_guard = ctx.borrow_guard();
                (self)($(<$T as Part<M>>::extract(ctx)),+)
            }
        }
    };
}

macro_rules! impl_event_handler_mut {
    ($($T:ident),+ $(,)?) => {
        #[allow(non_snake_case)]
        impl<E, X, P, M, F, $($T),+> EventHandler<E, X, P, M, ($(Mut<$T>,)+)> for F
        where
            F: for<'a> Fn(P, $(&'a mut $T),+) -> Command<E, X> + 'static,
            $($T: PartMut<M>,)+
        {
            #[track_caller]
            fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X> {
                let _borrow_guard = ctx.borrow_guard();
                (self)(payload, $(<$T as PartMut<M>>::extract_mut(ctx)),+)
            }
        }

        #[allow(non_snake_case)]
        impl<E, X, M, F, $($T),+> EventHandler<E, X, (), M, EventNP<($(Mut<$T>,)+)>> for F
        where
            F: for<'a> Fn($(&'a mut $T),+) -> Command<E, X> + 'static,
            $($T: PartMut<M>,)+
        {
            #[track_caller]
            fn handle(&self, _payload: (), ctx: &EventContext<M>) -> Command<E, X> {
                let _borrow_guard = ctx.borrow_guard();
                (self)($(<$T as PartMut<M>>::extract_mut(ctx)),+)
            }
        }
    };
}

impl_event_handler!(T1);
impl_event_handler!(T1, T2);
impl_event_handler!(T1, T2, T3);
impl_event_handler!(T1, T2, T3, T4);
impl_event_handler!(T1, T2, T3, T4, T5);
impl_event_handler!(T1, T2, T3, T4, T5, T6);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

impl_event_handler_mut!(T1);
impl_event_handler_mut!(T1, T2);
impl_event_handler_mut!(T1, T2, T3);
impl_event_handler_mut!(T1, T2, T3, T4);
impl_event_handler_mut!(T1, T2, T3, T4, T5);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6, T7);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_event_handler_mut!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(feature = "shell")]
impl_subscription_handler!(T1);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6, T7);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
#[cfg(feature = "shell")]
impl_subscription_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::{Arc, Mutex, OnceLock};

    use super::*;
    use crate as syzygy;
    use crate::command::CommandStep;
    use crate::test_store::TestStore;
    use futures::StreamExt;

    // ── shared test types ───────────────────────────────────────────

    #[derive(Debug, Clone)]
    enum Event {
        Saved,
        Incremented,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Log(String),
    }

    // ── Explicit model extraction generated by #[derive(Model)] ──────

    #[derive(crate::Model)]
    struct AppModel {
        #[model(wrapper = Counter)]
        counter: i32,
        #[model(wrapper = Name)]
        name: String,
    }

    #[derive(crate::Model)]
    struct ArityEventModel {
        #[model(wrapper = A1)]
        a1: i32,
        #[model(wrapper = A2)]
        a2: i32,
        #[model(wrapper = A3)]
        a3: i32,
        #[model(wrapper = A4)]
        a4: i32,
        #[model(wrapper = A5)]
        a5: i32,
        #[model(wrapper = A6)]
        a6: i32,
        #[model(wrapper = A7)]
        a7: i32,
        #[model(wrapper = A8)]
        a8: i32,
        #[model(wrapper = A9)]
        a9: i32,
        #[model(wrapper = A10)]
        a10: i32,
        #[model(wrapper = A11)]
        a11: i32,
        #[model(wrapper = A12)]
        a12: i32,
    }

    #[derive(crate::Model)]
    struct ZstBoundaryModel {
        #[model(wrapper = Marker)]
        marker: (),
        _payload: u8,
    }

    #[derive(Clone)]
    struct R1(u8);
    #[derive(Clone)]
    struct R2(u8);
    #[derive(Clone)]
    struct R3(u8);
    #[derive(Clone)]
    struct R4(u8);
    #[derive(Clone)]
    struct R5(u8);
    #[derive(Clone)]
    struct R6(u8);
    #[derive(Clone)]
    struct R7(u8);
    #[derive(Clone)]
    struct R8(u8);
    #[derive(Clone)]
    struct R9(u8);
    #[derive(Clone)]
    struct R10(u8);
    #[derive(Clone)]
    struct R11(u8);
    #[derive(Clone)]
    struct R12(u8);

    // ── Resources (for effect tests) ────────────────────────────────

    #[derive(Clone)]
    struct DbUrl(String);

    impl DbUrl {
        fn as_str(&self) -> &str {
            &self.0
        }
    }

    fn arity_resources() -> ResourceMap {
        let mut resources = ResourceMap::new();
        resources.insert(R1(1));
        resources.insert(R2(2));
        resources.insert(R3(3));
        resources.insert(R4(4));
        resources.insert(R5(5));
        resources.insert(R6(6));
        resources.insert(R7(7));
        resources.insert(R8(8));
        resources.insert(R9(9));
        resources.insert(R10(10));
        resources.insert(R11(11));
        resources.insert(R12(12));
        resources
    }

    fn capture_panic_location(f: impl FnOnce()) -> (String, u32) {
        static PANIC_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = PANIC_HOOK_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock.lock().expect("panic hook mutex must not be poisoned");

        let location = Arc::new(Mutex::new(None));
        let location_in_hook = Arc::clone(&location);
        let current_thread = std::thread::current().id();
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if std::thread::current().id() != current_thread {
                return;
            }
            let panic_location = info
                .location()
                .map(|location| (location.file().to_string(), location.line()));
            *location_in_hook
                .lock()
                .expect("panic location mutex must not be poisoned") = panic_location;
        }));

        let result = panic::catch_unwind(AssertUnwindSafe(f));
        panic::set_hook(previous_hook);
        assert!(
            result.is_err(),
            "expected panic, but closure completed successfully"
        );

        let captured_location = location
            .lock()
            .expect("panic location mutex must not be poisoned")
            .clone()
            .expect("panic hook should capture a location");

        captured_location
    }

    fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
        match payload.downcast::<String>() {
            Ok(message) => *message,
            Err(payload) => match payload.downcast::<&'static str>() {
                Ok(message) => (*message).to_string(),
                Err(_) => String::from("<non-string panic payload>"),
            },
        }
    }

    fn capture_panic_message_and_location(f: impl FnOnce()) -> (String, u32, String) {
        static PANIC_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = PANIC_HOOK_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock.lock().expect("panic hook mutex must not be poisoned");

        let location = Arc::new(Mutex::new(None));
        let location_in_hook = Arc::clone(&location);
        let current_thread = std::thread::current().id();
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if std::thread::current().id() != current_thread {
                return;
            }
            let panic_location = info
                .location()
                .map(|location| (location.file().to_string(), location.line()));
            *location_in_hook
                .lock()
                .expect("panic location mutex must not be poisoned") = panic_location;
        }));

        let result = panic::catch_unwind(AssertUnwindSafe(f));
        panic::set_hook(previous_hook);
        let panic_payload = result.expect_err("expected panic, but closure completed successfully");

        let captured_location = location
            .lock()
            .expect("panic location mutex must not be poisoned")
            .clone()
            .expect("panic hook should capture a location");
        let message = panic_payload_to_string(panic_payload);

        (captured_location.0, captured_location.1, message)
    }

    // ── Event handler tests ─────────────────────────────────────────

    #[test]
    fn event_one_field() {
        fn increment(amount: u32, counter: &mut Counter) -> Command<Event, Effect> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            **counter += amount;
            Command::event(Event::Incremented)
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        {
            let ctx = EventContext::new(&mut model);
            let _cmd = increment.handle(5, &ctx);
        }
        assert_eq!(model.counter, 5);
    }

    #[test]
    fn event_two_fields() {
        fn save(data: String, counter: &mut Counter, name: &mut Name) -> Command<Event, Effect> {
            **counter += 1;
            **name = data;
            Command::event(Event::Saved)
        }

        let mut model = AppModel {
            counter: 10,
            name: "old".into(),
        };
        {
            let ctx = EventContext::new(&mut model);
            let _cmd = save.handle("new".into(), &ctx);
        }
        assert_eq!(model.counter, 11);
        assert_eq!(model.name, "new");
    }

    #[test]
    fn event_no_model() {
        fn pure(_: ()) -> Command<Event, Effect> {
            Command::effect(Effect::Log("hello".into()))
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);
        let _cmd = pure.handle((), &ctx);
    }

    #[test]
    fn event_no_payload_handler_can_omit_unit_parameter() {
        fn pure() -> Command<Event, Effect> {
            Command::event(Event::Saved)
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);
        let cmd = pure.handle((), &ctx);

        assert_eq!(cmd.into_iter().count(), 1);
    }

    #[test]
    #[should_panic(expected = "already borrowed mutably")]
    fn double_borrow_panics() {
        fn bad(_: (), _c1: &mut Counter, _c2: &mut Counter) -> Command<Event, Effect> {
            unreachable!()
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);
        let _ = bad.handle((), &ctx);
    }

    #[test]
    fn event_extract_mut_single_field() {
        fn increment(amount: u32, counter: &mut Counter) -> Command<Event, Effect> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            **counter += amount;
            Command::none()
        }

        let mut model = AppModel {
            counter: 1,
            name: String::new(),
        };
        {
            let ctx = EventContext::new(&mut model);
            let _ = increment.handle(4, &ctx);
        }

        assert_eq!(model.counter, 5);
    }

    #[test]
    fn event_extract_mut_multiple_fields() {
        fn mutate(counter: &mut Counter, name: &mut Name) -> Command<Event, Effect> {
            **counter += 2;
            name.push_str("-updated");
            Command::none()
        }

        let mut model = AppModel {
            counter: 3,
            name: "state".to_string(),
        };
        {
            let ctx = EventContext::new(&mut model);
            let _ = mutate.handle((), &ctx);
        }

        assert_eq!(model.counter, 5);
        assert_eq!(model.name, "state-updated");
    }

    #[test]
    #[should_panic(expected = "already borrowed mutably")]
    fn event_extract_mut_double_borrow_panics() {
        fn bad(_: (), _first: &mut Counter, _second: &mut Counter) -> Command<Event, Effect> {
            unreachable!()
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);
        let _ = bad.handle((), &ctx);
    }

    #[test]
    fn direct_extract_mut_panics_at_callsite() {
        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };

        let expected_line = Cell::new(0_u32);
        let (file, line) = capture_panic_location(|| {
            let ctx = EventContext::new(&mut model);
            let _first = Counter::extract_mut(&ctx);
            expected_line.set(line!() + 1);
            let _second = Counter::extract_mut(&ctx);
        });

        assert!(file.ends_with("src/extract.rs"));
        assert_eq!(line, expected_line.get());
    }

    #[test]
    fn handler_extract_mut_panics_at_callsite() {
        fn bad(_: (), _first: &mut Counter, _second: &mut Counter) -> Command<Event, Effect> {
            unreachable!()
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };

        let expected_line = Cell::new(0_u32);
        let (file, line) = capture_panic_location(|| {
            let ctx = EventContext::new(&mut model);
            expected_line.set(line!() + 1);
            let _ = bad.handle((), &ctx);
        });

        assert!(file.ends_with("src/extract.rs"));
        assert_eq!(line, expected_line.get());
    }

    #[test]
    fn extraction_overlap_panic_includes_programmer_error_hint() {
        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };

        let (_file, _line, message) = capture_panic_message_and_location(|| {
            let ctx = EventContext::new(&mut model);
            let _first = Counter::extract_mut(&ctx);
            let _second = Counter::extract_mut(&ctx);
        });

        assert!(message.contains("already borrowed mutably"));
        assert!(message.contains("programmer error"));
        assert!(message.contains("overlapping borrows"));
    }

    #[test]
    #[should_panic(expected = "already borrowed mutably")]
    fn track_field_borrow_panics_on_overlap() {
        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);

        ctx.track_field_borrow(0, "counter");
        ctx.track_field_borrow(0, "counter_again");
    }

    #[test]
    fn track_field_borrow_supports_256_indices() {
        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);

        for index in 0..256 {
            ctx.track_field_borrow(index, "field");
        }
    }

    #[test]
    #[should_panic(expected = "exceeds borrow tracker capacity (256)")]
    fn track_field_borrow_rejects_index_256() {
        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);

        ctx.track_field_borrow(256, "out_of_bounds");
    }

    #[test]
    fn event_handle_match() {
        fn increment(amount: u32, counter: &mut Counter) -> Command<Event, Effect> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            **counter += amount;
            Command::none()
        }

        fn rename(new_name: String, name: &mut Name) -> Command<Event, Effect> {
            **name = new_name;
            Command::none()
        }

        #[derive(Debug, Clone)]
        enum Ev {
            Increment(u32),
            Rename(String),
        }

        let handle_event = |event: Ev, ctx: &EventContext<AppModel>| match event {
            Ev::Increment(n) => increment.handle(n, ctx),
            Ev::Rename(s) => rename.handle(s, ctx),
        };

        let mut model = AppModel {
            counter: 0,
            name: "old".into(),
        };

        {
            let ctx = EventContext::new(&mut model);
            handle_event(Ev::Increment(3), &ctx);
        }
        assert_eq!(model.counter, 3);

        {
            let ctx = EventContext::new(&mut model);
            handle_event(Ev::Rename("new".into()), &ctx);
        }
        assert_eq!(model.name, "new");
    }

    #[test]
    fn sequential_mut_handler_calls_can_reborrow_same_field() {
        fn increment_once(_: (), counter: &mut Counter) -> Command<Event, Effect> {
            **counter += 1;
            Command::none()
        }

        fn handle_event(_: (), ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
            increment_once
                .handle((), ctx)
                .and(increment_once.handle((), ctx))
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);
        let _ = handle_event((), &ctx);

        assert_eq!(model.counter, 2);
    }

    #[test]
    fn event_handler_mut_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        fn mutate(
            delta: i32,
            a1: &mut A1,
            a2: &mut A2,
            a3: &mut A3,
            a4: &mut A4,
            a5: &mut A5,
            a6: &mut A6,
            a7: &mut A7,
            a8: &mut A8,
            a9: &mut A9,
            a10: &mut A10,
            a11: &mut A11,
            a12: &mut A12,
        ) -> Command<Event, Effect> {
            **a1 += delta;
            **a2 += delta;
            **a3 += delta;
            **a4 += delta;
            **a5 += delta;
            **a6 += delta;
            **a7 += delta;
            **a8 += delta;
            **a9 += delta;
            **a10 += delta;
            **a11 += delta;
            **a12 += delta;
            Command::none()
        }

        let mut model = ArityEventModel {
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
        };

        let ctx = EventContext::new(&mut model);
        let _ = mutate.handle(3, &ctx);

        assert_eq!(model.a1, 3);
        assert_eq!(model.a2, 3);
        assert_eq!(model.a3, 3);
        assert_eq!(model.a4, 3);
        assert_eq!(model.a5, 3);
        assert_eq!(model.a6, 3);
        assert_eq!(model.a7, 3);
        assert_eq!(model.a8, 3);
        assert_eq!(model.a9, 3);
        assert_eq!(model.a10, 3);
        assert_eq!(model.a11, 3);
        assert_eq!(model.a12, 3);
    }

    #[test]
    fn event_handler_np_mut_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        fn mutate(
            a1: &mut A1,
            a2: &mut A2,
            a3: &mut A3,
            a4: &mut A4,
            a5: &mut A5,
            a6: &mut A6,
            a7: &mut A7,
            a8: &mut A8,
            a9: &mut A9,
            a10: &mut A10,
            a11: &mut A11,
            a12: &mut A12,
        ) -> Command<Event, Effect> {
            **a1 = 1;
            **a2 = 2;
            **a3 = 3;
            **a4 = 4;
            **a5 = 5;
            **a6 = 6;
            **a7 = 7;
            **a8 = 8;
            **a9 = 9;
            **a10 = 10;
            **a11 = 11;
            **a12 = 12;
            Command::none()
        }

        let mut model = ArityEventModel {
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
        };

        let ctx = EventContext::new(&mut model);
        let _ = mutate.handle((), &ctx);

        assert_eq!(model.a1, 1);
        assert_eq!(model.a2, 2);
        assert_eq!(model.a3, 3);
        assert_eq!(model.a4, 4);
        assert_eq!(model.a5, 5);
        assert_eq!(model.a6, 6);
        assert_eq!(model.a7, 7);
        assert_eq!(model.a8, 8);
        assert_eq!(model.a9, 9);
        assert_eq!(model.a10, 10);
        assert_eq!(model.a11, 11);
        assert_eq!(model.a12, 12);
    }

    #[test]
    fn event_handler_owned_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        fn read(
            _: (),
            a1: &A1,
            a2: &A2,
            a3: &A3,
            a4: &A4,
            a5: &A5,
            a6: &A6,
            a7: &A7,
            a8: &A8,
            a9: &A9,
            a10: &A10,
            a11: &A11,
            a12: &A12,
        ) -> Command<Event, Effect> {
            let sum = **a1
                + **a2
                + **a3
                + **a4
                + **a5
                + **a6
                + **a7
                + **a8
                + **a9
                + **a10
                + **a11
                + **a12;
            assert_eq!(sum, 78);
            Command::none()
        }

        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };
        let ctx = EventContext::new(&mut model);
        let _ = read.handle((), &ctx);
    }

    #[test]
    fn event_handler_np_owned_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        fn read(
            a1: &A1,
            a2: &A2,
            a3: &A3,
            a4: &A4,
            a5: &A5,
            a6: &A6,
            a7: &A7,
            a8: &A8,
            a9: &A9,
            a10: &A10,
            a11: &A11,
            a12: &A12,
        ) -> Command<Event, Effect> {
            let sum = **a1
                + **a2
                + **a3
                + **a4
                + **a5
                + **a6
                + **a7
                + **a8
                + **a9
                + **a10
                + **a11
                + **a12;
            assert_eq!(sum, 78);
            Command::none()
        }

        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };
        let ctx = EventContext::new(&mut model);
        let _ = read.handle((), &ctx);
    }

    #[test]
    fn whole_model_can_be_extracted_immutably() {
        fn read_model(_: (), model: &ArityEventModel) -> Command<Event, Effect> {
            let sum = model.a1
                + model.a2
                + model.a3
                + model.a4
                + model.a5
                + model.a6
                + model.a7
                + model.a8
                + model.a9
                + model.a10
                + model.a11
                + model.a12;
            assert_eq!(sum, 78);
            Command::none()
        }

        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };

        let ctx = EventContext::new(&mut model);
        let _ = read_model.handle((), &ctx);
    }

    #[test]
    fn whole_model_can_be_extracted_mutably() {
        fn mutate_model(_: (), model: &mut ArityEventModel) -> Command<Event, Effect> {
            model.a1 = 42;
            Command::none()
        }

        let mut model = ArityEventModel {
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
        };

        let ctx = EventContext::new(&mut model);
        let _ = mutate_model.handle((), &ctx);

        assert_eq!(model.a1, 42);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed mutable region")]
    fn whole_model_mut_then_field_mut_panics() {
        let mut model = ArityEventModel {
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
        };

        let ctx = EventContext::new(&mut model);
        let _model = <ArityEventModel as PartMut<ArityEventModel>>::extract_mut(&ctx);
        let _field = A1::extract_mut(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed mutable region")]
    fn whole_model_mut_then_field_immut_panics() {
        let mut model = ArityEventModel {
            a1: 0,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
        };

        let ctx = EventContext::new(&mut model);
        let _model = <ArityEventModel as PartMut<ArityEventModel>>::extract_mut(&ctx);
        let _field = <A1 as Part<ArityEventModel>>::extract(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed immutable region")]
    fn field_immut_then_field_mut_panics() {
        let mut model = ArityEventModel {
            a1: 1,
            a2: 0,
            a3: 0,
            a4: 0,
            a5: 0,
            a6: 0,
            a7: 0,
            a8: 0,
            a9: 0,
            a10: 0,
            a11: 0,
            a12: 0,
        };

        let ctx = EventContext::new(&mut model);
        let _field_ref = <A1 as Part<ArityEventModel>>::extract(&ctx);
        let _field_mut = A1::extract_mut(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed immutable region")]
    fn field_immut_then_whole_model_mut_panics() {
        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };

        let ctx = EventContext::new(&mut model);
        let _field_ref = <A1 as Part<ArityEventModel>>::extract(&ctx);
        let _model_mut = <ArityEventModel as PartMut<ArityEventModel>>::extract_mut(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed immutable region")]
    fn whole_model_immut_then_whole_model_mut_panics() {
        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };

        let ctx = EventContext::new(&mut model);
        let _model_ref = <ArityEventModel as Part<ArityEventModel>>::extract(&ctx);
        let _model_mut = <ArityEventModel as PartMut<ArityEventModel>>::extract_mut(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed mutable region")]
    fn field_mut_then_whole_model_immut_panics() {
        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };

        let ctx = EventContext::new(&mut model);
        let _field_mut = A1::extract_mut(&ctx);
        let _model_ref = <ArityEventModel as Part<ArityEventModel>>::extract(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed mutable region")]
    fn field_mut_then_field_immut_panics() {
        let mut model = ArityEventModel {
            a1: 1,
            a2: 2,
            a3: 3,
            a4: 4,
            a5: 5,
            a6: 6,
            a7: 7,
            a8: 8,
            a9: 9,
            a10: 10,
            a11: 11,
            a12: 12,
        };

        let ctx = EventContext::new(&mut model);
        let _field_mut = A1::extract_mut(&ctx);
        let _field_ref = <A1 as Part<ArityEventModel>>::extract(&ctx);
    }

    #[test]
    #[should_panic(expected = "overlaps already-borrowed mutable region")]
    fn zst_boundary_overlap_is_rejected() {
        let mut model = ZstBoundaryModel {
            marker: (),
            _payload: 7,
        };

        let ctx = EventContext::new(&mut model);
        let _model = <ZstBoundaryModel as PartMut<ZstBoundaryModel>>::extract_mut(&ctx);
        let _marker = Marker::extract_mut(&ctx);
    }

    // ── Effect handler tests ────────────────────────────────────────

    #[test]
    fn effect_handle() {
        fn save(data: String, db: DbUrl) -> Task<Event, Effect> {
            assert_eq!(data, "x");
            assert_eq!(db.as_str(), "pg://test");
            Task::send(Event::Saved)
        }

        fn log(msg: String) -> Task<Event, Effect> {
            let _ = msg;
            Task::none()
        }

        let mut resources = ResourceMap::new();
        resources.insert(DbUrl("pg://test".into()));
        let ctx = EffectContext::new(&resources);
        let handle_effect = |effect: Effect, ctx: &EffectContext<'_>| match effect {
            Effect::Log(msg) => log.handle(msg, ctx),
        };
        let _ = save.handle("x".into(), &ctx);
        let _ = handle_effect(Effect::Log("hi".into()), &ctx);
    }

    #[test]
    fn effect_async_handler_infers_future_marker() {
        async fn save(data: String, db: DbUrl) -> Command<Event, Effect> {
            assert_eq!(data, "x");
            assert_eq!(db.as_str(), "pg://test");
            Command::event(Event::Saved)
        }

        let mut resources = ResourceMap::new();
        resources.insert(DbUrl("pg://test".into()));
        let ctx = EffectContext::new(&resources);

        match save.handle("x".to_string(), &ctx) {
            Task::Future(future) => {
                let command = futures::executor::block_on(future);
                assert_eq!(command.into_iter().count(), 1);
            }
            _ => panic!("expected Task::Future for async effect handler"),
        }
    }

    #[test]
    fn effect_stream_handler_infers_stream_marker() {
        fn watch(_: (), db: DbUrl) -> impl Stream<Item = Command<Event, Effect>> {
            let first = db.as_str().to_string();
            futures::stream::iter([
                Command::event(Event::Saved),
                Command::effect(Effect::Log(first)),
            ])
        }

        let mut resources = ResourceMap::new();
        resources.insert(DbUrl("pg://test".into()));
        let ctx = EffectContext::new(&resources);

        match watch.handle((), &ctx) {
            Task::Stream(stream) => {
                let commands = futures::executor::block_on(stream.collect::<Vec<_>>());
                assert_eq!(commands.len(), 2);
            }
            _ => panic!("expected Task::Stream for stream effect handler"),
        }
    }

    #[test]
    fn effect_task_handler_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        fn run(
            payload: u8,
            r1: R1,
            r2: R2,
            r3: R3,
            r4: R4,
            r5: R5,
            r6: R6,
            r7: R7,
            r8: R8,
            r9: R9,
            r10: R10,
            r11: R11,
            r12: R12,
        ) -> Task<Event, Effect> {
            let sum = payload
                + r1.0
                + r2.0
                + r3.0
                + r4.0
                + r5.0
                + r6.0
                + r7.0
                + r8.0
                + r9.0
                + r10.0
                + r11.0
                + r12.0;
            assert_eq!(sum, 79);
            Task::none()
        }

        let resources = arity_resources();
        let ctx = EffectContext::new(&resources);
        let _ = run.handle(1, &ctx);
    }

    #[test]
    fn effect_future_handler_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        async fn run(
            payload: u8,
            r1: R1,
            r2: R2,
            r3: R3,
            r4: R4,
            r5: R5,
            r6: R6,
            r7: R7,
            r8: R8,
            r9: R9,
            r10: R10,
            r11: R11,
            r12: R12,
        ) -> Command<Event, Effect> {
            let sum = payload
                + r1.0
                + r2.0
                + r3.0
                + r4.0
                + r5.0
                + r6.0
                + r7.0
                + r8.0
                + r9.0
                + r10.0
                + r11.0
                + r12.0;
            assert_eq!(sum, 79);
            Command::none()
        }

        let resources = arity_resources();
        let ctx = EffectContext::new(&resources);

        match run.handle(1, &ctx) {
            Task::Future(future) => {
                let command = futures::executor::block_on(future);
                assert!(command.is_empty());
            }
            _ => panic!("expected Task::Future for 12-arg future effect handler"),
        }
    }

    #[test]
    fn effect_stream_handler_supports_twelve_extractors() {
        #[allow(clippy::too_many_arguments)]
        fn run(
            payload: u8,
            r1: R1,
            r2: R2,
            r3: R3,
            r4: R4,
            r5: R5,
            r6: R6,
            r7: R7,
            r8: R8,
            r9: R9,
            r10: R10,
            r11: R11,
            r12: R12,
        ) -> impl Stream<Item = Command<Event, Effect>> {
            let sum = payload
                + r1.0
                + r2.0
                + r3.0
                + r4.0
                + r5.0
                + r6.0
                + r7.0
                + r8.0
                + r9.0
                + r10.0
                + r11.0
                + r12.0;
            assert_eq!(sum, 79);
            futures::stream::iter([Command::none()])
        }

        let resources = arity_resources();
        let ctx = EffectContext::new(&resources);

        match run.handle(1, &ctx) {
            Task::Stream(stream) => {
                let commands = futures::executor::block_on(stream.collect::<Vec<_>>());
                assert_eq!(commands.len(), 1);
                assert!(commands[0].is_empty());
            }
            _ => panic!("expected Task::Stream for 12-arg stream effect handler"),
        }
    }

    // ── Scope composition tests ─────────────────────────────────────

    #[derive(Debug, Default, crate::Model)]
    struct ScopedCounterModel {
        #[model(wrapper = Count)]
        count: i32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ScopedCounterEvent {
        Increment(i32),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ScopedCounterEffect {
        Log(String),
    }

    fn scoped_counter_increment(
        amount: i32,
        count: &mut Count,
    ) -> Command<ScopedCounterEvent, ScopedCounterEffect> {
        **count += amount;
        Command::effect(ScopedCounterEffect::Log(format!("count={}", **count)))
    }

    fn scoped_counter_handle_event(
        event: ScopedCounterEvent,
        ctx: &EventContext<ScopedCounterModel>,
    ) -> Command<ScopedCounterEvent, ScopedCounterEffect> {
        match event {
            ScopedCounterEvent::Increment(amount) => scoped_counter_increment.handle(amount, ctx),
        }
    }

    #[derive(Debug, Default, crate::Model)]
    struct ScopedToggleModel {
        #[model(wrapper = Enabled)]
        enabled: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ScopedToggleEvent {
        Set(bool),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ScopedToggleEffect {
        Changed(bool),
    }

    fn scoped_toggle_set(
        enabled: bool,
        current: &mut Enabled,
    ) -> Command<ScopedToggleEvent, ScopedToggleEffect> {
        **current = enabled;
        Command::effect(ScopedToggleEffect::Changed(**current))
    }

    fn scoped_toggle_handle_event(
        event: ScopedToggleEvent,
        ctx: &EventContext<ScopedToggleModel>,
    ) -> Command<ScopedToggleEvent, ScopedToggleEffect> {
        match event {
            ScopedToggleEvent::Set(enabled) => scoped_toggle_set.handle(enabled, ctx),
        }
    }

    #[derive(Debug, Default, crate::Model)]
    struct ScopedAppModel {
        #[model(wrapper = Title)]
        title: String,
        #[model(part)]
        counter: ScopedCounterModel,
        #[model(part)]
        toggle: ScopedToggleModel,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ScopedAppEvent {
        Counter(ScopedCounterEvent),
        Toggle(ScopedToggleEvent),
        Rename(String),
        CounterWithRename { amount: i32, title: String },
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ScopedAppEffect {
        Counter(ScopedCounterEffect),
        Toggle(ScopedToggleEffect),
    }

    fn scoped_app_handle_event(
        event: ScopedAppEvent,
        ctx: &EventContext<ScopedAppModel>,
    ) -> Command<ScopedAppEvent, ScopedAppEffect> {
        match event {
            ScopedAppEvent::Counter(child_event) => {
                let counter = ScopedCounterModel::extract_mut(ctx);
                let child_ctx = EventContext::new(counter);
                scoped_counter_handle_event(child_event, &child_ctx)
                    .map_event(ScopedAppEvent::Counter)
                    .map_effect(ScopedAppEffect::Counter)
            }
            ScopedAppEvent::Toggle(child_event) => {
                let toggle = ScopedToggleModel::extract_mut(ctx);
                let child_ctx = EventContext::new(toggle);
                scoped_toggle_handle_event(child_event, &child_ctx)
                    .map_event(ScopedAppEvent::Toggle)
                    .map_effect(ScopedAppEffect::Toggle)
            }
            ScopedAppEvent::Rename(new_title) => {
                let title = Title::extract_mut(ctx);
                **title = new_title;
                Command::none()
            }
            ScopedAppEvent::CounterWithRename {
                amount,
                title: new_title,
            } => {
                {
                    let title = Title::extract_mut(ctx);
                    **title = new_title;
                }

                let counter = ScopedCounterModel::extract_mut(ctx);
                let child_ctx = EventContext::new(counter);
                scoped_counter_increment
                    .handle(amount, &child_ctx)
                    .map_event(ScopedAppEvent::Counter)
                    .map_effect(ScopedAppEffect::Counter)
            }
        }
    }

    #[derive(Clone)]
    struct ScopedDbUrl(String);

    impl ScopedDbUrl {
        fn as_str(&self) -> &str {
            &self.0
        }
    }

    fn scoped_save(_: (), db_url: ScopedDbUrl) -> Task<ScopedCounterEvent, ScopedCounterEffect> {
        assert_eq!(db_url.as_str(), "pg://scope");
        Task::none()
    }

    #[test]
    fn event_scope_updates_child_model() {
        let mut model = ScopedAppModel::default();
        let ctx = EventContext::new(&mut model);

        let _ = scoped_app_handle_event(
            ScopedAppEvent::Counter(ScopedCounterEvent::Increment(4)),
            &ctx,
        );

        assert_eq!(model.counter.count, 4);
    }

    #[test]
    fn scoped_command_mapping_wraps_child_effects() {
        let mut model = ScopedAppModel::default();
        let ctx = EventContext::new(&mut model);

        let command = scoped_app_handle_event(
            ScopedAppEvent::Counter(ScopedCounterEvent::Increment(2)),
            &ctx,
        );

        assert_eq!(
            command.into_iter().collect::<Vec<_>>(),
            vec![CommandStep::Effect(ScopedAppEffect::Counter(
                ScopedCounterEffect::Log("count=2".to_string())
            ))]
        );
    }

    #[test]
    fn scoped_context_borrow_tracking_is_independent() {
        let mut model = ScopedAppModel::default();
        let ctx = EventContext::new(&mut model);

        let _ = scoped_app_handle_event(
            ScopedAppEvent::CounterWithRename {
                amount: 3,
                title: "scoped".to_string(),
            },
            &ctx,
        );

        assert_eq!(model.title, "scoped");
        assert_eq!(model.counter.count, 3);
    }

    #[test]
    fn parent_composes_multiple_children() {
        let mut model = ScopedAppModel::default();

        {
            let ctx = EventContext::new(&mut model);
            let _ = scoped_app_handle_event(
                ScopedAppEvent::Counter(ScopedCounterEvent::Increment(5)),
                &ctx,
            );
        }

        {
            let ctx = EventContext::new(&mut model);
            let _ =
                scoped_app_handle_event(ScopedAppEvent::Toggle(ScopedToggleEvent::Set(true)), &ctx);
        }

        assert_eq!(model.counter.count, 5);
        assert!(model.toggle.enabled);
    }

    #[test]
    fn test_store_handles_scoped_parent_handle_event() {
        let mut store = TestStore::new(ScopedAppModel::default(), scoped_app_handle_event);

        store.send(ScopedAppEvent::Counter(ScopedCounterEvent::Increment(3)));
        assert_eq!(store.state().counter.count, 3);
        store.assert_effects([ScopedAppEffect::Counter(ScopedCounterEffect::Log(
            "count=3".to_string(),
        ))]);

        store.send(ScopedAppEvent::Toggle(ScopedToggleEvent::Set(true)));
        assert!(store.state().toggle.enabled);
        store.assert_effects([ScopedAppEffect::Toggle(ScopedToggleEffect::Changed(true))]);

        store.send(ScopedAppEvent::Rename("updated".to_string()));
        assert_eq!(store.state().title, "updated");
        store.assert_no_effects();
    }

    #[test]
    fn effect_context_extracts_registered_resources() {
        let mut resources = ResourceMap::new();
        resources.insert(ScopedDbUrl("pg://scope".to_string()));
        let ctx = EffectContext::new(&resources);

        let _ = scoped_save.handle((), &ctx);
    }

    #[test]
    fn missing_effect_resource_panics_with_type_hint_and_callsite() {
        #[derive(Clone)]
        struct MissingResource;

        let resources = ResourceMap::new();
        let ctx = EffectContext::new(&resources);
        let expected_line = Cell::new(0_u32);
        let (file, line, message) = capture_panic_message_and_location(|| {
            expected_line.set(line!() + 1);
            let _: MissingResource = MissingResource::from_context(&ctx);
        });

        assert!(file.ends_with("src/extract.rs"));
        assert!(line > 0);
        assert!(message.contains("MissingResource"));
        assert!(message.contains("Register it with .with_resource()"));
        assert!(message.contains("programmer error"));
        assert!(message.contains("src/extract.rs"));
        assert!(message.contains(&expected_line.get().to_string()));
    }

    // ── End-to-end with builder ─────────────────────────────────────

    #[cfg(feature = "shell")]
    #[test]
    fn end_to_end() {
        use crate::prelude::*;

        #[derive(Debug, Clone)]
        enum Ev {
            Increment(u32),
            Rename(String),
        }

        #[derive(Debug, Clone)]
        struct Fx;

        fn increment(amount: u32, counter: &mut Counter) -> Command<Ev, Fx> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            **counter += amount;
            Command::none()
        }

        fn rename(new_name: String, name: &mut Name) -> Command<Ev, Fx> {
            **name = new_name;
            Command::none()
        }

        let mut runner = Syzygy::builder::<Ev, Fx>()
            .model(AppModel {
                counter: 0,
                name: "init".into(),
            })
            .event_handler(|event: Ev, ctx: &EventContext<AppModel>| match event {
                Ev::Increment(n) => increment.handle(n, ctx),
                Ev::Rename(s) => rename.handle(s, ctx),
            })
            .effect_handler(|_effect: Fx, _ctx: &EffectContext<'_>| Task::<Ev, Fx>::none())
            .build()
            .unwrap();

        runner.core().try_send(Ev::Increment(7)).unwrap();
        runner.step().unwrap();
        assert_eq!(runner.model().counter, 7);

        runner.core().try_send(Ev::Rename("hello".into())).unwrap();
        runner.step().unwrap();
        assert_eq!(runner.model().name, "hello");
    }
}
