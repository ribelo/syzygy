use crate::effect_context::EffectContext;
use crate::executor::Task;

/// Sync effect handler producing an `Task` plan.
pub type EffectHandler<E, X, R> = fn(effect: X, ctx: &EffectContext<E, R>) -> Task<E, R>;


