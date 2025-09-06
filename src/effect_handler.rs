use crate::effect_context::EffectContext;
use crate::streaming::EffectResult;
use std::future::Future;
use std::pin::Pin;

/// Update function type that takes an event and a mutable EventContext
pub type EffectHandler<E, X, R> = fn(
    effect: X,
    ctx: &mut EffectContext<R>,
) -> Pin<Box<dyn Future<Output = EffectResult<E>> + Send>>;
