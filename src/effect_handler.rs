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

/// Boxed effect handler trait for simpler Shell types
/// 
/// This allows Shell to avoid being generic over the handler type H,
/// making it much easier to work with at the cost of one dynamic dispatch per effect.
pub trait BoxedEffectHandler<
    Event,
    Effect, 
    Resources,
    Executors = crate::executor::EmptyExecutorStorage,
>: Send + Sync
{
    fn handle_boxed(
        &self,
        effect: Effect,
        ctx: EffectContext<Event, Resources, Executors>,
    ) -> std::pin::Pin<Box<dyn Future<Output = EffectOutput<Event>> + Send + 'static>>;
}

/// Implement BoxedEffectHandler for any EffectHandler
impl<H, Event, Effect, Resources, Executors> BoxedEffectHandler<Event, Effect, Resources, Executors> for H
where
    H: EffectHandler<Event, Effect, Resources, Executors>,
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Send + Sync + 'static,
    Executors: Send + Sync + 'static,
{
    fn handle_boxed(
        &self,
        effect: Effect,
        ctx: EffectContext<Event, Resources, Executors>,
    ) -> std::pin::Pin<Box<dyn Future<Output = EffectOutput<Event>> + Send + 'static>> {
        Box::pin(self.handle(effect, ctx))
    }
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
