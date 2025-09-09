use futures_util::future::{BoxFuture, FutureExt, join_all, select_all};

use std::future::Future;

use crate::prelude::EffectContext;
use crate::streaming::EffectOutput;

use super::ExecutorError;
use std::any::TypeId;

/// Object-safe factory to start a task using an EffectContext.
///
/// This allows storing `FnOnce(EffectContext<E,R>) -> Fut` as a trait object
/// by wrapping it in a struct holding `Option<F>`.
type FutFactory<E, R> =
    Box<dyn FnOnce(EffectContext<E, R>) -> BoxFuture<'static, EffectOutput<E>> + Send>;

type SyncFactory<E, R> = Box<dyn FnOnce(EffectContext<E, R>) -> EffectOutput<E> + Send>;

/// Declarative effect plan produced by sync handlers.
pub enum EffectPlan<E, R> {
    Events(Vec<E>),
    Future {
        exec: TypeId,
        task: FutFactory<E, R>,
    },

    /// Run a synchronous task on a Sync executor (e.g., Rayon, single-thread)
    Sync {
        exec: TypeId,
        task: SyncFactory<E, R>,
    },
    Race(Vec<EffectPlan<E, R>>),
    All(Vec<EffectPlan<E, R>>),
}

impl<E, R> EffectPlan<E, R>
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    #[must_use]
    pub fn events(events: Vec<E>) -> Self {
        Self::Events(events)
    }

    fn future<F, Fut>(exec: TypeId, f: F) -> Self
    where
        F: FnOnce(EffectContext<E, R>) -> Fut + Send + 'static,
        Fut: Future<Output = EffectOutput<E>> + Send + 'static,
    {
        let task: FutFactory<E, R> = Box::new(move |ctx| async move { f(ctx).await }.boxed());
        Self::Future { exec, task }
    }



    /// Create a Sync task spec from a closure that returns immediately.
    fn sync<F>(exec: TypeId, f: F) -> Self
    where
        F: FnOnce(EffectContext<E, R>) -> EffectOutput<E> + Send + 'static,
    {
        let task: SyncFactory<E, R> = Box::new(f);
        Self::Sync { exec, task }
    }

    /// Future on a marker type key
    pub fn future_on<T: 'static, F, Fut>(f: F) -> Self
    where
        F: FnOnce(EffectContext<E, R>) -> Fut + Send + 'static,
        Fut: Future<Output = EffectOutput<E>> + Send + 'static,
    {
        Self::future(TypeId::of::<T>(), f)
    }



    /// Sync on a marker type key
    pub fn sync_on<T: 'static, F>(f: F) -> Self
    where
        F: FnOnce(EffectContext<E, R>) -> EffectOutput<E> + Send + 'static,
    {
        Self::sync(TypeId::of::<T>(), f)
    }

    #[must_use]
    pub fn race(branches: Vec<EffectPlan<E, R>>) -> Self {
        Self::Race(branches)
    }
    #[must_use]
    pub fn all(branches: Vec<EffectPlan<E, R>>) -> Self {
        Self::All(branches)
    }
}

/// Drive an EffectSpec by spawning appropriate tasks on executors and
/// forwarding produced events to Core via the supplied `EffectContext`.
pub fn drive_spec<E, R>(spec: EffectPlan<E, R>, ctx: EffectContext<E, R>) -> BoxFuture<'static, ()>
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    async move {
        use crate::streaming::consume_effect_output;

        match spec {
            EffectPlan::Events(events) => {
                for e in events { ctx.send_event(e); }
            }
            EffectPlan::Future { exec, task } => {
                if let Some(exec_ref) = ctx.async_executor_by_typeid(exec) {
                    let fut = (task)(ctx.clone());
                    match exec_ref.spawn_future(fut).await {
                        Ok(output) => consume_effect_output(output, &ctx).await,
                        Err(ExecutorError::WorkerGone | ExecutorError::Panic { msg: _ } | ExecutorError::Cancelled) => { /* ignore or log */ }
                    }
                }
            }

            EffectPlan::Race(branches) => {
                // Spawn all branches and select the first to complete
                let pending: Vec<_> = branches
                    .into_iter()
                    .map(|b| drive_spec(b, ctx.clone()))
                    .collect();

                if !pending.is_empty() {
                    let (_winner, _idx, _rest) = select_all(pending).await;
                    // Dropping _rest cancels losers
                }
            }
            EffectPlan::All(branches) => {
                let futs: Vec<_> = branches.into_iter().map(|b| drive_spec(b, ctx.clone())).collect();
                let _ = join_all(futs).await;
            }
            EffectPlan::Sync { exec, task } => {
                if let Some(exec_ref) = ctx.sync_executor_by_typeid(exec) {
                    let ctx_for_job = ctx.clone();
                    let job = Box::new(move || (task)(ctx_for_job));
                    match exec_ref.spawn_sync(job).await {
                        Ok(output) => consume_effect_output(output, &ctx).await,
                        Err(ExecutorError::WorkerGone | ExecutorError::Panic { msg: _ } | ExecutorError::Cancelled) => { /* ignore or log */ }
                    }
                }
            }
        }
    }
    .boxed()
}
