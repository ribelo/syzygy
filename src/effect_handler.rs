use crate::effect_context::EffectContext;
use crate::executor::EffectPlan;
use crate::executor::spec::drive_spec;
use std::future::Future;
use std::pin::Pin;

/// Sync effect handler producing an `EffectPlan` plan.
pub type EffectHandler<E, X, R> = fn(effect: X, ctx: &EffectContext<E, R>) -> EffectPlan<E, R>;

/// Back-compat boxed trait used by Shell/Builder to erase effect handler type.
pub trait BoxedEffectHandler<E, X, R>: Send + Sync + 'static {
    /// Build the plan synchronously and return a future that drives it.
    fn handle_boxed(
        &self,
        effect: X,
        ctx: EffectContext<E, R>,
    ) -> Pin<Box<dyn Future<Output = crate::streaming::EffectOutput<E>> + Send>>;
}

impl<E, X, R> BoxedEffectHandler<E, X, R> for EffectHandler<E, X, R>
where
    E: Clone + Send + 'static,
    X: Clone + Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    fn handle_boxed(
        &self,
        effect: X,
        ctx: EffectContext<E, R>,
    ) -> Pin<Box<dyn Future<Output = crate::streaming::EffectOutput<E>> + Send>> {
        let spec = (self)(effect, &ctx);
        Box::pin(async move {
            drive_spec(spec, ctx).await;
            crate::streaming::EffectOutput::None
        })
    }
}
