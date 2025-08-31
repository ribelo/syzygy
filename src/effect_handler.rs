use crate::async_context::EffectContext;
use crate::streaming::EffectOutput;
use std::future::Future;

/// Zero-cost effect handler trait (AFIT-friendly)
///
/// - Handler itself is generic (not boxed)
/// - Returned future is a concrete type (no boxing per call)
pub trait EffectHandler<
    Event,
    Effect,
    Resources,
    Executors = crate::executor::EmptyExecutorStorage,
>: Send + Sync + Clone + Copy + 'static
{
    type Future: Future<Output = EffectOutput<Event>> + Send + 'static;

    fn handle(
        &self,
        effect: Effect,
        ctx: EffectContext<Event, Resources, Executors>,
    ) -> Self::Future;
}

impl<Event, Effect, Resources, Executors, Fut, F>
    EffectHandler<Event, Effect, Resources, Executors> for F
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Send + Sync + 'static,
    Executors: Send + Sync + 'static,
    F: Fn(Effect, EffectContext<Event, Resources, Executors>) -> Fut + Send + Sync + Clone + Copy + 'static,
    Fut: Future<Output = EffectOutput<Event>> + Send + 'static,
{
    type Future = Fut;

    fn handle(
        &self,
        effect: Effect,
        ctx: EffectContext<Event, Resources, Executors>,
    ) -> Self::Future {
        (self)(effect, ctx)
    }
}

impl<Event, Effect, Resources, Executors> EffectHandler<Event, Effect, Resources, Executors> for ()
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Send + Sync + 'static,
    Executors: Send + Sync + 'static,
{
    type Future = std::future::Ready<EffectOutput<Event>>;

    fn handle(
        &self,
        _effect: Effect,
        _ctx: EffectContext<Event, Resources, Executors>,
    ) -> Self::Future {
        std::future::ready(crate::streaming::EffectOutput::None)
    }
}
