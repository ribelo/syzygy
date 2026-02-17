use crate::executor::Task;

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

pub trait EffectHandler<E: Send + 'static, X: Send + 'static, P, R, Marker>: Send + 'static {
    fn handle(&self, payload: P, ctx: &EffectContext<R>) -> Task<E, X>;
}

// 0 extractors: fn(P) -> Task
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
impl_effect_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_effect_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_effect_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_effect_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Resources {
        db_url: String,
        retry_count: u32,
    }

    #[derive(Clone)]
    struct DbUrl(String);

    impl FromEffectContext<Resources> for DbUrl {
        fn from_context(ctx: &EffectContext<Resources>) -> Self {
            DbUrl(ctx.resources().db_url.clone())
        }
    }

    #[derive(Clone)]
    struct RetryCount(u32);

    impl FromEffectContext<Resources> for RetryCount {
        fn from_context(ctx: &EffectContext<Resources>) -> Self {
            RetryCount(ctx.resources().retry_count)
        }
    }

    #[derive(Debug, Clone)]
    enum Event {
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Save(String),
        Notify(String),
    }

    #[test]
    fn zero_extractors() {
        fn save(_data: String) -> Task<Event, Effect> {
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = save.handle("hello".into(), &ctx);
    }

    #[test]
    fn one_extractor() {
        fn save(data: String, db: DbUrl) -> Task<Event, Effect> {
            assert_eq!(data, "hello");
            assert_eq!(db.0, "pg://localhost");
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = save.handle("hello".into(), &ctx);
    }

    #[test]
    fn two_extractors() {
        fn save(data: String, db: DbUrl, retries: RetryCount) -> Task<Event, Effect> {
            assert_eq!(data, "hello");
            assert_eq!(db.0, "pg://localhost");
            assert_eq!(retries.0, 3);
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = save.handle("hello".into(), &ctx);
    }

    #[test]
    fn dispatch_match() {
        fn save(data: String, db: DbUrl) -> Task<Event, Effect> {
            assert_eq!(data, "hello");
            assert_eq!(db.0, "pg://localhost");
            Task::event(Event::Done)
        }

        fn notify(msg: String) -> Task<Event, Effect> {
            assert_eq!(msg, "world");
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });

        let dispatch = |effect: Effect, ctx: &EffectContext<Resources>| match effect {
            Effect::Save(data) => save.handle(data, ctx),
            Effect::Notify(msg) => notify.handle(msg, ctx),
        };

        let _task = dispatch(Effect::Save("hello".into()), &ctx);
        let _task = dispatch(Effect::Notify("world".into()), &ctx);
    }

    #[cfg(feature = "shell")]
    #[test]
    fn end_to_end_with_builder() {
        use crate::prelude::*;

        #[derive(Default)]
        struct Model {
            saved: bool,
        }

        fn save(data: String, db: DbUrl) -> Task<Event, Effect> {
            assert_eq!(data, "test_data");
            assert_eq!(db.0, "pg://test");
            Task::event(Event::Done)
        }

        fn notify(msg: String) -> Task<Event, Effect> {
            let _ = msg;
            Task::none()
        }

        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .with_resources(Resources {
                db_url: "pg://test".into(),
                retry_count: 5,
            })
            .event_handler(|_event: Event, model: &mut Model| -> Command<Event, Effect> {
                model.saved = true;
                Command::none()
            })
            .effect_handler(|effect: Effect, ctx: &EffectContext<Resources>| match effect {
                Effect::Save(data) => save.handle(data, ctx),
                Effect::Notify(msg) => notify.handle(msg, ctx),
            })
            .with_async_executor(InlineAsync::new())
            .build();

        runner.core().try_send_event(Event::Done).unwrap();
        runner.step().unwrap();
        assert!(runner.model().saved);
    }
}
