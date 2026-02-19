use std::cell::Cell;

use crate::command::Command;
use crate::executor::Task;

// ── Effect side ─────────────────────────────────────────────────────

pub struct EffectContext<R> {
    resources: R,
}

impl<R> EffectContext<R> {
    pub fn new(resources: R) -> Self {
        Self { resources }
    }

    pub fn resources(&self) -> &R {
        &self.resources
    }
}

pub trait FromEffectContext<R> {
    fn from_context(ctx: &EffectContext<R>) -> Self;
}

pub trait EffectHandler<E: Send + 'static, X: Send + 'static, P, R, Marker>:
    Send + 'static
{
    fn handle(&self, payload: P, ctx: &EffectContext<R>) -> Task<E, X>;
}

impl<E, X, P, R, F> EffectHandler<E, X, P, R, ()> for F
where
    F: Fn(P) -> Task<E, X> + Send + 'static,
    E: Send + 'static,
    X: Send + 'static,
{
    fn handle(&self, payload: P, _ctx: &EffectContext<R>) -> Task<E, X> {
        (self)(payload)
    }
}

macro_rules! impl_effect_handler {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, R, F, $($T),+> EffectHandler<E, X, P, R, ($($T,)+)> for F
        where
            F: Fn(P, $($T),+) -> Task<E, X> + Send + 'static,
            $($T: FromEffectContext<R>,)+
            E: Send + 'static,
            X: Send + 'static,
        {
            fn handle(&self, payload: P, ctx: &EffectContext<R>) -> Task<E, X> {
                (self)(payload, $($T::from_context(ctx)),+)
            }
        }
    }
}

impl_effect_handler!(T1);
impl_effect_handler!(T1, T2);
impl_effect_handler!(T1, T2, T3);
impl_effect_handler!(T1, T2, T3, T4);
impl_effect_handler!(T1, T2, T3, T4, T5);
impl_effect_handler!(T1, T2, T3, T4, T5, T6);
impl_effect_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_effect_handler!(T1, T2, T3, T4, T5, T6, T7, T8);

// ── Event side ──────────────────────────────────────────────────────

pub struct EventContext<M> {
    ptr: *mut M,
    borrowed: Cell<u64>,
}

impl<M> EventContext<M> {
    pub(crate) fn new(model: &mut M) -> Self {
        Self {
            ptr: model as *mut M,
            borrowed: Cell::new(0),
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

    /// # Safety
    /// Caller must have called `track_borrow` for this field first, and the
    /// field offset must be correct for type `T` within `M`.
    pub unsafe fn field_ptr<T>(&self, offset: usize) -> *mut T {
        self.ptr.cast::<u8>().add(offset).cast::<T>()
    }

    pub fn model_ptr(&self) -> *mut M {
        self.ptr
    }
}

pub trait FromEventContext<M> {
    fn from_context(ctx: &EventContext<M>) -> Self;
}

pub trait EventHandler<E: Send + 'static, X: Send + 'static, P, M, Marker>: Send + 'static {
    fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X>;
}

impl<E, X, P, M, F> EventHandler<E, X, P, M, ()> for F
where
    F: Fn(P) -> Command<E, X> + Send + 'static,
    E: Send + 'static,
    X: Send + 'static,
{
    fn handle(&self, payload: P, _ctx: &EventContext<M>) -> Command<E, X> {
        (self)(payload)
    }
}

macro_rules! impl_event_handler {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, P, M, F, $($T),+> EventHandler<E, X, P, M, ($($T,)+)> for F
        where
            F: Fn(P, $($T),+) -> Command<E, X> + Send + 'static,
            $($T: FromEventContext<M>,)+
            E: Send + 'static,
            X: Send + 'static,
        {
            fn handle(&self, payload: P, ctx: &EventContext<M>) -> Command<E, X> {
                (self)(payload, $($T::from_context(ctx)),+)
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

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── Model + hand-written "derive" output ────────────────────────

    struct AppModel {
        counter: i32,
        name: String,
    }

    // -- Counter field wrapper (derive would generate this) --

    pub struct Counter(*mut i32);

    impl std::ops::Deref for Counter {
        type Target = i32;
        fn deref(&self) -> &i32 {
            // SAFETY: Counter is only constructed from a valid `AppModel::counter` pointer.
            unsafe { &*self.0 }
        }
    }
    impl std::ops::DerefMut for Counter {
        fn deref_mut(&mut self) -> &mut i32 {
            // SAFETY: Counter provides unique mutable access tracked by `EventContext::track_borrow`.
            unsafe { &mut *self.0 }
        }
    }

    impl FromEventContext<AppModel> for Counter {
        fn from_context(ctx: &EventContext<AppModel>) -> Self {
            ctx.track_borrow(0, "counter");
            // SAFETY: `track_borrow` enforces single mutable access to this field for the handler call.
            Counter(unsafe { &mut (*ctx.model_ptr()).counter })
        }
    }

    // -- Name field wrapper (derive would generate this) --

    pub struct Name(*mut String);

    impl std::ops::Deref for Name {
        type Target = String;
        fn deref(&self) -> &String {
            // SAFETY: Name is only constructed from a valid `AppModel::name` pointer.
            unsafe { &*self.0 }
        }
    }
    impl std::ops::DerefMut for Name {
        fn deref_mut(&mut self) -> &mut String {
            // SAFETY: Name provides unique mutable access tracked by `EventContext::track_borrow`.
            unsafe { &mut *self.0 }
        }
    }

    impl FromEventContext<AppModel> for Name {
        fn from_context(ctx: &EventContext<AppModel>) -> Self {
            ctx.track_borrow(1, "name");
            // SAFETY: `track_borrow` enforces single mutable access to this field for the handler call.
            Name(unsafe { &mut (*ctx.model_ptr()).name })
        }
    }

    // ── Resources (for effect tests) ────────────────────────────────

    #[derive(Clone)]
    struct Resources {
        db_url: String,
    }

    #[derive(Clone)]
    struct DbUrl(String);

    impl FromEffectContext<Resources> for DbUrl {
        fn from_context(ctx: &EffectContext<Resources>) -> Self {
            DbUrl(ctx.resources().db_url.clone())
        }
    }

    // ── Event handler tests ─────────────────────────────────────────

    #[test]
    fn event_one_field() {
        fn increment(amount: u32, mut counter: Counter) -> Command<Event, Effect> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            *counter += amount;
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
        fn save(data: String, mut counter: Counter, mut name: Name) -> Command<Event, Effect> {
            *counter += 1;
            *name = data;
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
        fn bad(_: (), _c1: Counter, _c2: Counter) -> Command<Event, Effect> {
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
    fn event_dispatch_match() {
        fn increment(amount: u32, mut counter: Counter) -> Command<Event, Effect> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            *counter += amount;
            Command::none()
        }

        fn rename(new_name: String, mut name: Name) -> Command<Event, Effect> {
            *name = new_name;
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

    // ── Effect handler tests ────────────────────────────────────────

    #[test]
    fn effect_dispatch() {
        fn save(data: String, db: DbUrl) -> Task<Event, Effect> {
            assert_eq!(data, "x");
            assert_eq!(db.0, "pg://test");
            Task::event(Event::Saved)
        }

        fn log(msg: String) -> Task<Event, Effect> {
            let _ = msg;
            Task::none()
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://test".into(),
        });
        let dispatch = |effect: Effect, ctx: &EffectContext<Resources>| match effect {
            Effect::Log(msg) => log.handle(msg, ctx),
        };
        let _ = save.handle("x".into(), &ctx);
        let _ = dispatch(Effect::Log("hi".into()), &ctx);
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

        fn increment(amount: u32, mut counter: Counter) -> Command<Ev, Fx> {
            let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
            *counter += amount;
            Command::none()
        }

        fn rename(new_name: String, mut name: Name) -> Command<Ev, Fx> {
            *name = new_name;
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
            .effect_handler(|_effect: Fx, _ctx: &EffectContext<()>| Task::<Ev, Fx>::none())
            .with_async_executor(InlineAsync::new())
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
