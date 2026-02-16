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

pub trait EffectHandler<E: Send + 'static, X: Send + 'static, R, Marker>: Send + 'static {
    fn call(&self, effect: X, ctx: &EffectContext<R>) -> Task<E, X>;
}

// 0 extractors: fn(X) -> Task
impl<E, X, R, F> EffectHandler<E, X, R, ()> for F
where
    F: Fn(X) -> Task<E, X> + Send + 'static,
    E: Send + 'static,
    X: Send + 'static,
{
    fn call(&self, effect: X, _ctx: &EffectContext<R>) -> Task<E, X> {
        (self)(effect)
    }
}

macro_rules! impl_effect_handler {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<E, X, R, F, $($T),+> EffectHandler<E, X, R, ($($T,)+)> for F
        where
            F: Fn(X, $($T),+) -> Task<E, X> + Send + 'static,
            $($T: FromEffectContext<R>,)+
            E: Send + 'static,
            X: Send + 'static,
        {
            fn call(&self, effect: X, ctx: &EffectContext<R>) -> Task<E, X> {
                (self)(effect, $($T::from_context(ctx)),+)
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
        Save(#[allow(dead_code)] String),
    }

    #[test]
    fn zero_extractors() {
        fn handler(_effect: Effect) -> Task<Event, Effect> {
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task: Task<Event, Effect> =
            EffectHandler::<Event, Effect, Resources, ()>::call(&handler, Effect::Save("x".into()), &ctx);
    }

    #[test]
    fn one_extractor() {
        fn handler(_effect: Effect, db: DbUrl) -> Task<Event, Effect> {
            assert_eq!(db.0, "pg://localhost");
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = EffectHandler::<Event, Effect, Resources, (DbUrl,)>::call(
            &handler,
            Effect::Save("x".into()),
            &ctx,
        );
    }

    #[test]
    fn two_extractors() {
        fn handler(_effect: Effect, db: DbUrl, retries: RetryCount) -> Task<Event, Effect> {
            assert_eq!(db.0, "pg://localhost");
            assert_eq!(retries.0, 3);
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = EffectHandler::<Event, Effect, Resources, (DbUrl, RetryCount)>::call(
            &handler,
            Effect::Save("x".into()),
            &ctx,
        );
    }

    #[cfg(feature = "shell")]
    #[test]
    fn end_to_end_with_builder() {
        use crate::prelude::*;

        #[derive(Default)]
        struct Model {
            saved: bool,
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
            .effect_handler(|effect: Effect, db: DbUrl, retries: RetryCount| -> Task<Event, Effect> {
                assert_eq!(db.0, "pg://test");
                assert_eq!(retries.0, 5);
                match effect {
                    Effect::Save(_) => Task::event(Event::Done),
                }
            })
            .with_async_executor(InlineAsync::new())
            .build();

        runner.core().try_send_event(Event::Done).unwrap();
        runner.step().unwrap();
        assert!(runner.model().saved);
    }

    #[test]
    fn closure_handler() {
        let prefix = "LOG".to_string();
        let handler = move |_effect: Effect, db: DbUrl| -> Task<Event, Effect> {
            let _ = &prefix;
            assert_eq!(db.0, "pg://localhost");
            Task::event(Event::Done)
        };

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = EffectHandler::<Event, Effect, Resources, (DbUrl,)>::call(
            &handler,
            Effect::Save("x".into()),
            &ctx,
        );
    }

    impl FromEffectContext<Resources> for Resources {
        fn from_context(ctx: &EffectContext<Resources>) -> Self {
            ctx.resources().clone()
        }
    }

    #[test]
    fn extract_whole_resources() {
        fn handler(_effect: Effect, res: Resources) -> Task<Event, Effect> {
            assert_eq!(res.db_url, "pg://localhost");
            Task::event(Event::Done)
        }

        let ctx = EffectContext::new(Resources {
            db_url: "pg://localhost".into(),
            retry_count: 3,
        });
        let _task = EffectHandler::<Event, Effect, Resources, (Resources,)>::call(
            &handler,
            Effect::Save("x".into()),
            &ctx,
        );
    }
}
