use std::cell::{Cell, RefCell};
use std::future::Future;

use futures::Stream;

use crate::command::Command;
use crate::dependency::{Resource, ResourceMap};
use crate::executor::Task;

// ── Effect side ─────────────────────────────────────────────────────

pub struct EffectContext {
    resources: ResourceMap,
}

impl EffectContext {
    #[must_use]
    pub fn new(resources: ResourceMap) -> Self {
        Self { resources }
    }

    #[must_use]
    pub fn resources(&self) -> &ResourceMap {
        &self.resources
    }
}

pub trait FromEffectContext {
    fn from_context(ctx: &EffectContext) -> Self;
}

impl<T: Resource> FromEffectContext for T {
    fn from_context(ctx: &EffectContext) -> Self {
        ctx.resources().get::<T>().unwrap_or_else(|| {
            panic!(
                "Resource `{}` not found in ResourceMap. Register it with .with_resource()",
                std::any::type_name::<T>()
            )
        })
    }
}

pub struct FutureEffect;
pub struct StreamEffect;

#[doc(hidden)]
pub struct NP;

pub trait EffectHandler<E: 'static, X: 'static, P, Marker>: 'static {
    fn handle(&self, payload: P, ctx: &EffectContext) -> Task<E, X>;
}

impl<E, X, P, F> EffectHandler<E, X, P, ()> for F
where
    F: Fn(P) -> Task<E, X> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext) -> Task<E, X> {
        (self)(payload)
    }
}

impl<E, X, F> EffectHandler<E, X, (), NP> for F
where
    F: Fn() -> Task<E, X> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, _payload: (), _ctx: &EffectContext) -> Task<E, X> {
        (self)()
    }
}

impl<E, X, P, F, Fut> EffectHandler<E, X, P, FutureEffect> for F
where
    F: Fn(P) -> Fut + 'static,
    Fut: Future<Output = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext) -> Task<E, X> {
        Task::future((self)(payload))
    }
}

impl<E, X, F, Fut> EffectHandler<E, X, (), (FutureEffect, NP)> for F
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, _payload: (), _ctx: &EffectContext) -> Task<E, X> {
        Task::future((self)())
    }
}

impl<E, X, P, F, S> EffectHandler<E, X, P, StreamEffect> for F
where
    F: Fn(P) -> S + 'static,
    S: Stream<Item = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext) -> Task<E, X> {
        Task::stream((self)(payload))
    }
}

impl<E, X, F, S> EffectHandler<E, X, (), (StreamEffect, NP)> for F
where
    F: Fn() -> S + 'static,
    S: Stream<Item = Command<E, X>> + 'static,
    E: 'static,
    X: 'static,
{
    fn handle(&self, _payload: (), _ctx: &EffectContext) -> Task<E, X> {
        Task::stream((self)())
    }
}

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
            fn handle(&self, payload: P, ctx: &EffectContext) -> Task<E, X> {
                (self)(payload, $($T::from_context(ctx)),+)
            }
        }
    }
}

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
            fn handle(&self, payload: P, ctx: &EffectContext) -> Task<E, X> {
                Task::future((self)(payload, $($T::from_context(ctx)),+))
            }
        }
    }
}

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
            fn handle(&self, payload: P, ctx: &EffectContext) -> Task<E, X> {
                Task::stream((self)(payload, $($T::from_context(ctx)),+))
            }
        }
    }
}

impl_effect_handler_task!(T1);
impl_effect_handler_task!(T1, T2);
impl_effect_handler_task!(T1, T2, T3);
impl_effect_handler_task!(T1, T2, T3, T4);

impl_effect_handler_future!(T1);
impl_effect_handler_future!(T1, T2);
impl_effect_handler_future!(T1, T2, T3);
impl_effect_handler_future!(T1, T2, T3, T4);

impl_effect_handler_stream!(T1);
impl_effect_handler_stream!(T1, T2);
impl_effect_handler_stream!(T1, T2, T3);
impl_effect_handler_stream!(T1, T2, T3, T4);

// ── Event side ──────────────────────────────────────────────────────

pub struct EventContext<M> {
    ptr: *mut M,
    borrowed: Cell<u64>,
    borrowed_ranges: RefCell<Vec<(usize, usize)>>,
}

impl<M> EventContext<M> {
    pub(crate) fn new(model: &mut M) -> Self {
        Self {
            ptr: model as *mut M,
            borrowed: Cell::new(0),
            borrowed_ranges: RefCell::new(Vec::new()),
        }
    }

    pub fn track_borrow(&self, field_index: u32, field_name: &str) {
        let mask = 1u64 << field_index;
        let current = self.borrowed.get();
        assert!(
            current & mask == 0,
            "field '{field_name}' (index {field_index}) already borrowed mutably in this handler"
        );
        self.borrowed.set(current | mask);
    }

    pub fn track_borrow_range(&self, ptr: *mut u8, size: usize, field_name: &str) {
        let start = ptr as usize;
        let end = start.checked_add(size).unwrap_or_else(|| {
            panic!("field '{field_name}' produced an overflow while tracking mutable borrow range")
        });
        let mut borrowed_ranges = self.borrowed_ranges.borrow_mut();

        for &(borrowed_start, borrowed_end) in borrowed_ranges.iter() {
            let disjoint = start >= borrowed_end || end <= borrowed_start;
            assert!(
                disjoint,
                "field '{field_name}' overlaps already-borrowed mutable region"
            );
        }

        borrowed_ranges.push((start, end));
    }

    pub(crate) fn borrow_guard(&self) -> BorrowGuard<'_, M> {
        BorrowGuard {
            ctx: self,
            borrowed_bits: self.borrowed.get(),
            borrowed_ranges_len: self.borrowed_ranges.borrow().len(),
        }
    }

    /// # Safety
    /// Caller must have called `track_borrow` for this field first, and the
    /// field offset must be correct for type `T` within `M`.
    pub unsafe fn field_ptr<T>(&self, offset: usize) -> *mut T {
        self.ptr.cast::<u8>().add(offset).cast::<T>()
    }

    pub fn model_ptr(&self) -> *mut M {
        self.ptr
    }

    pub fn dispatch<E, X, H, Marker>(&self, handler: H) -> Command<E, X>
    where
        H: EventHandler<E, X, (), M, Marker>,
    {
        handler.handle((), self)
    }
}

pub(crate) struct BorrowGuard<'a, M> {
    ctx: &'a EventContext<M>,
    borrowed_bits: u64,
    borrowed_ranges_len: usize,
}

impl<M> Drop for BorrowGuard<'_, M> {
    fn drop(&mut self) {
        self.ctx.borrowed.set(self.borrowed_bits);
        self.ctx
            .borrowed_ranges
            .borrow_mut()
            .truncate(self.borrowed_ranges_len);
    }
}

pub trait FromEventContext<M> {
    fn from_context(ctx: &EventContext<M>) -> Self;
}

#[allow(clippy::mut_from_ref)]
pub trait ExtractMutFrom<M> {
    fn extract_mut(ctx: &EventContext<M>) -> &mut Self;
}

pub trait EventHandler<E, X, P, M, Marker>: 'static {
    fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X>;
}

impl<E, X, P, M, F> EventHandler<E, X, P, M, ()> for F
where
    F: Fn(P) -> Command<E, X> + 'static,
{
    fn handle(&self, payload: P, _ctx: &EventContext<M>) -> Command<E, X> {
        (self)(payload)
    }
}

#[doc(hidden)]
pub struct Owned<T>(::core::marker::PhantomData<fn() -> T>);

macro_rules! impl_event_handler {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, M, F, $($T),+> EventHandler<E, X, P, M, ($(Owned<$T>,)+)> for F
        where
            F: Fn(P, $($T),+) -> Command<E, X> + 'static,
            $($T: FromEventContext<M>,)+
        {
            fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X> {
                (self)(payload, $($T::from_context(ctx)),+)
            }
        }
    }
}

#[doc(hidden)]
pub struct Mut<T>(::core::marker::PhantomData<fn(&mut T)>);

#[doc(hidden)]
pub struct EventNP<Inner>(::core::marker::PhantomData<fn() -> Inner>);

macro_rules! impl_event_handler_mut {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, M, F, $($T),+> EventHandler<E, X, P, M, ($(Mut<$T>,)+)> for F
        where
            F: for<'a> Fn(P, $(&'a mut $T),+) -> Command<E, X> + 'static,
            $($T: ExtractMutFrom<M>,)+
        {
            fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X> {
                let _borrow_guard = ctx.borrow_guard();
                (self)(payload, $($T::extract_mut(ctx)),+)
            }
        }
    }
}

macro_rules! impl_event_handler_np_owned {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, M, F, $($T),+> EventHandler<E, X, (), M, EventNP<($(Owned<$T>,)+)>> for F
        where
            F: Fn($($T),+) -> Command<E, X> + 'static,
            $($T: FromEventContext<M>,)+
        {
            fn handle(&self, _payload: (), ctx: &EventContext<M>) -> Command<E, X> {
                (self)($($T::from_context(ctx)),+)
            }
        }
    }
}

macro_rules! impl_event_handler_np_mut {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, M, F, $($T),+> EventHandler<E, X, (), M, EventNP<($(Mut<$T>,)+)>> for F
        where
            F: for<'a> Fn($(&'a mut $T),+) -> Command<E, X> + 'static,
            $($T: ExtractMutFrom<M>,)+
        {
            fn handle(&self, _payload: (), ctx: &EventContext<M>) -> Command<E, X> {
                let _borrow_guard = ctx.borrow_guard();
                (self)($($T::extract_mut(ctx)),+)
            }
        }
    }
}

impl_event_handler!(T1);
impl_event_handler!(T1, T2);
impl_event_handler!(T1, T2, T3);
impl_event_handler!(T1, T2, T3, T4);
impl_event_handler!(T1, T2, T3, T4, T5);
impl_event_handler!(T1, T2, T3, T4, T5, T6);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_event_handler!(T1, T2, T3, T4, T5, T6, T7, T8);

impl_event_handler_mut!(T1);
impl_event_handler_mut!(T1, T2);
impl_event_handler_mut!(T1, T2, T3);
impl_event_handler_mut!(T1, T2, T3, T4);

impl_event_handler_np_owned!(T1);
impl_event_handler_np_owned!(T1, T2);
impl_event_handler_np_owned!(T1, T2, T3);
impl_event_handler_np_owned!(T1, T2, T3, T4);

impl_event_handler_np_mut!(T1);
impl_event_handler_np_mut!(T1, T2);
impl_event_handler_np_mut!(T1, T2, T3);
impl_event_handler_np_mut!(T1, T2, T3, T4);

#[cfg(test)]
mod tests {
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

    // ── Model wrappers generated by #[derive(Model)] ─────────────────

    #[derive(crate::Model)]
    struct AppModel {
        counter: i32,
        name: String,
    }

    // ── Resources (for effect tests) ────────────────────────────────

    #[derive(Clone)]
    struct DbUrl(String);

    impl DbUrl {
        fn as_str(&self) -> &str {
            &self.0
        }
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
    #[should_panic(expected = "overlaps already-borrowed mutable region")]
    fn track_borrow_range_panics_on_overlap() {
        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);

        let ptr = std::ptr::addr_of_mut!(model.counter).cast::<u8>();
        ctx.track_borrow_range(ptr, std::mem::size_of::<i32>(), "counter");
        ctx.track_borrow_range(ptr, std::mem::size_of::<i32>(), "counter_again");
    }

    #[test]
    fn event_dispatch_match() {
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

        let dispatch = |event: Ev, ctx: &EventContext<AppModel>| match event {
            Ev::Increment(n) => increment.handle(n, ctx),
            Ev::Rename(s) => rename.handle(s, ctx),
        };

        let mut model = AppModel {
            counter: 0,
            name: "old".into(),
        };

        {
            let ctx = EventContext::new(&mut model);
            dispatch(Ev::Increment(3), &ctx);
        }
        assert_eq!(model.counter, 3);

        {
            let ctx = EventContext::new(&mut model);
            dispatch(Ev::Rename("new".into()), &ctx);
        }
        assert_eq!(model.name, "new");
    }

    #[test]
    fn sequential_mut_handler_calls_can_reborrow_same_field() {
        fn increment_once(_: (), counter: &mut Counter) -> Command<Event, Effect> {
            **counter += 1;
            Command::none()
        }

        fn dispatch(_: (), ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
            increment_once
                .handle((), ctx)
                .and(increment_once.handle((), ctx))
        }

        let mut model = AppModel {
            counter: 0,
            name: String::new(),
        };
        let ctx = EventContext::new(&mut model);
        let _ = dispatch((), &ctx);

        assert_eq!(model.counter, 2);
    }

    // ── Effect handler tests ────────────────────────────────────────

    #[test]
    fn effect_dispatch() {
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
        let ctx = EffectContext::new(resources);
        let dispatch = |effect: Effect, ctx: &EffectContext| match effect {
            Effect::Log(msg) => log.handle(msg, ctx),
        };
        let _ = save.handle("x".into(), &ctx);
        let _ = dispatch(Effect::Log("hi".into()), &ctx);
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
        let ctx = EffectContext::new(resources);

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
        let ctx = EffectContext::new(resources);

        match watch.handle((), &ctx) {
            Task::Stream(stream) => {
                let commands = futures::executor::block_on(stream.collect::<Vec<_>>());
                assert_eq!(commands.len(), 2);
            }
            _ => panic!("expected Task::Stream for stream effect handler"),
        }
    }

    // ── Scope composition tests ─────────────────────────────────────

    #[derive(Debug, Default, crate::Model)]
    struct ScopedCounterModel {
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

    fn scoped_counter_dispatch(
        event: ScopedCounterEvent,
        ctx: &EventContext<ScopedCounterModel>,
    ) -> Command<ScopedCounterEvent, ScopedCounterEffect> {
        match event {
            ScopedCounterEvent::Increment(amount) => scoped_counter_increment.handle(amount, ctx),
        }
    }

    #[derive(Debug, Default, crate::Model)]
    struct ScopedToggleModel {
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

    fn scoped_toggle_dispatch(
        event: ScopedToggleEvent,
        ctx: &EventContext<ScopedToggleModel>,
    ) -> Command<ScopedToggleEvent, ScopedToggleEffect> {
        match event {
            ScopedToggleEvent::Set(enabled) => scoped_toggle_set.handle(enabled, ctx),
        }
    }

    #[derive(Debug, Default, crate::Model)]
    struct ScopedAppModel {
        title: String,
        #[extract]
        counter: ScopedCounterModel,
        #[extract]
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

    fn scoped_app_dispatch(
        event: ScopedAppEvent,
        ctx: &EventContext<ScopedAppModel>,
    ) -> Command<ScopedAppEvent, ScopedAppEffect> {
        match event {
            ScopedAppEvent::Counter(child_event) => {
                let counter = ScopedCounterModel::extract_mut(ctx);
                let child_ctx = EventContext::new(counter);
                scoped_counter_dispatch(child_event, &child_ctx)
                    .map_event(ScopedAppEvent::Counter)
                    .map_effect(ScopedAppEffect::Counter)
            }
            ScopedAppEvent::Toggle(child_event) => {
                let toggle = ScopedToggleModel::extract_mut(ctx);
                let child_ctx = EventContext::new(toggle);
                scoped_toggle_dispatch(child_event, &child_ctx)
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

        let _ = scoped_app_dispatch(
            ScopedAppEvent::Counter(ScopedCounterEvent::Increment(4)),
            &ctx,
        );

        assert_eq!(model.counter.count, 4);
    }

    #[test]
    fn scoped_command_mapping_wraps_child_effects() {
        let mut model = ScopedAppModel::default();
        let ctx = EventContext::new(&mut model);

        let command = scoped_app_dispatch(
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

        let _ = scoped_app_dispatch(
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
            let _ = scoped_app_dispatch(
                ScopedAppEvent::Counter(ScopedCounterEvent::Increment(5)),
                &ctx,
            );
        }

        {
            let ctx = EventContext::new(&mut model);
            let _ = scoped_app_dispatch(ScopedAppEvent::Toggle(ScopedToggleEvent::Set(true)), &ctx);
        }

        assert_eq!(model.counter.count, 5);
        assert!(model.toggle.enabled);
    }

    #[test]
    fn test_store_handles_scoped_parent_dispatch() {
        let mut store = TestStore::new(ScopedAppModel::default(), scoped_app_dispatch);

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
        let ctx = EffectContext::new(resources);

        let _ = scoped_save.handle((), &ctx);
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
            .effect_handler(|_effect: Fx, _ctx: &EffectContext| Task::<Ev, Fx>::none())
            .build();

        runner.core().try_send_event(Ev::Increment(7)).unwrap();
        runner.step().unwrap();
        assert_eq!(runner.model().counter, 7);

        runner
            .core()
            .try_send_event(Ev::Rename("hello".into()))
            .unwrap();
        runner.step().unwrap();
        assert_eq!(runner.model().name, "hello");
    }
}
