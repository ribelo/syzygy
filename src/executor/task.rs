//! Declarative task plans returned by effect handlers.
//!
//! A `Task<E, X>` describes what work to schedule. The Shell interprets the
//! plan and routes resulting `Command` steps back into the Core/Shell pipeline.
use std::any::Any;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender as EffectSender;
use futures_util::future::{BoxFuture, Either, FutureExt};
use futures_util::stream::{BoxStream, StreamExt};

use crate::activity::Activity;
use crate::command::{Command, CommandStep};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::{panic_message, AsyncExecutor, AsyncRt, BlockingExecutor};
use crate::shell::{prepare_effect_step_for_queue, CancelGenerationMap, CancelGuard, ShellStats};

fn missing_executor(kind: &'static str) -> ShellError {
    #[cfg(feature = "tracing")]
    tracing::error!(kind, "Missing executor; effect could not be scheduled");

    ShellError::TaskSpawnFailed(format!("no {kind} executor configured"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicTaskKind {
    Async,
    Compute,
    Blocking,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanicDetails {
    pub task_kind: PanicTaskKind,
}

impl PanicDetails {
    #[must_use]
    pub fn new(task_kind: PanicTaskKind) -> Self {
        Self { task_kind }
    }
}

pub type PanicHook<E, X> = dyn Fn(PanicDetails, String) -> Command<E, X> + Send + Sync;
type BlockingJob<E, X> = dyn FnOnce() -> Command<E, X> + Send;

#[derive(Clone)]
pub(crate) struct TaskRouting<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    pub activity: Option<Activity>,
    pub stats: Option<ShellStats>,
    pub cancel_generations: Option<CancelGenerationMap>,
    pub cancel_guard: Option<CancelGuard>,
    pub panic_handler: Option<Arc<PanicHook<E, X>>>,
}

impl<E, X> Default for TaskRouting<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    fn default() -> Self {
        Self {
            activity: None,
            stats: None,
            cancel_generations: None,
            cancel_guard: None,
            panic_handler: None,
        }
    }
}

fn command_from_panic<E, X>(
    panic_handler: Option<&Arc<PanicHook<E, X>>>,
    details: PanicDetails,
    payload: Box<dyn Any + Send>,
) -> Command<E, X> {
    let message = panic_message(payload);
    #[cfg(feature = "tracing")]
    tracing::error!(?details, %message, "Task panicked");
    if let Some(handler) = panic_handler {
        handler(details, message)
    } else {
        Command::none()
    }
}

/// Declarative unit of work returned by effect handlers.
pub enum Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    None,
    Resolved(Command<E, X>),
    Async {
        factory: Box<dyn FnOnce(AsyncRt) -> BoxFuture<'static, Command<E, X>> + Send>,
    },
    Compute {
        job: Box<BlockingJob<E, X>>,
    },
    Blocking {
        job: Box<BlockingJob<E, X>>,
    },
    Stream {
        factory: Box<dyn FnOnce(AsyncRt) -> BoxStream<'static, E> + Send>,
    },
}

impl<E, X> Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// No-op task. Useful when effects are conditionally skipped.
    #[must_use]
    pub fn none() -> Self {
        Self::None
    }

    /// Emit a single event back to Core.
    pub fn send(event: E) -> Self {
        Self::Resolved(Command::event(event))
    }

    /// Resolve to an already-built command.
    pub fn resolved(command: Command<E, X>) -> Self {
        Self::Resolved(command)
    }

    /// Run a future on the configured async executor.
    pub fn future<F, Fut>(factory: F) -> Self
    where
        F: FnOnce(AsyncRt) -> Fut + Send + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        Self::Async {
            factory: Box::new(move |rt| Box::pin(factory(rt)) as BoxFuture<'static, Command<E, X>>),
        }
    }

    pub fn run<F, Fut>(factory: F) -> Self
    where
        F: FnOnce(AsyncRt) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self::future(move |rt| async move {
            factory(rt).await;
            Command::none()
        })
    }

    /// Run a future after waiting for the provided delay.
    pub fn delayed<F, Fut>(duration: Duration, factory: F) -> Self
    where
        F: FnOnce(AsyncRt) -> Fut + Send + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        Self::future(move |rt| async move {
            rt.sleep(duration).await;
            factory(rt).await
        })
    }

    /// Emit a single event after waiting for the provided delay.
    pub fn delayed_send(duration: Duration, event: E) -> Self {
        Self::delayed(duration, move |_rt| async move { Command::event(event) })
    }

    /// Run a future with cancellation support.
    pub fn future_with_cancel<F, Fut, Cancel>(
        factory: F,
        cancel: Cancel,
        cancel_command: Command<E, X>,
    ) -> Self
    where
        F: FnOnce(AsyncRt) -> Fut + Send + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
        Cancel: Future<Output = ()> + Send + 'static,
    {
        Self::future(move |rt| {
            let future = factory(rt);
            async move {
                futures_util::pin_mut!(future);
                futures_util::pin_mut!(cancel);
                match futures_util::future::select(cancel, future).await {
                    Either::Left(((), _)) => cancel_command,
                    Either::Right((command, _)) => command,
                }
            }
        })
    }

    /// Forward stream items as events.
    pub fn stream<F, S>(factory: F) -> Self
    where
        F: FnOnce(AsyncRt) -> S + Send + 'static,
        S: futures_util::stream::Stream<Item = E> + Send + 'static,
    {
        Self::Stream {
            factory: Box::new(move |rt| Box::pin(factory(rt)) as BoxStream<'static, E>),
        }
    }

    /// Run CPU-bound work.
    pub fn compute<F>(job: F) -> Self
    where
        F: FnOnce() -> Command<E, X> + Send + 'static,
    {
        Self::Compute { job: Box::new(job) }
    }

    /// Run blocking I/O work.
    pub fn blocking<F>(job: F) -> Self
    where
        F: FnOnce() -> Command<E, X> + Send + 'static,
    {
        Self::Blocking { job: Box::new(job) }
    }

    /// Transform event and effect output types.
    #[must_use]
    pub fn map<E2, X2>(
        self,
        fe: impl Fn(E) -> E2 + Send + 'static,
        fx: impl Fn(X) -> X2 + Send + 'static,
    ) -> Task<E2, X2>
    where
        E2: Send + 'static,
        X2: Send + 'static,
    {
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => Task::Resolved(command.map(&fe, &fx)),
            Self::Async { factory } => Task::Async {
                factory: Box::new(move |rt| {
                    let future = factory(rt);
                    future.map(move |command| command.map(&fe, &fx)).boxed()
                }),
            },
            Self::Stream { factory } => Task::Stream {
                factory: Box::new(move |rt| {
                    let stream = factory(rt);
                    stream.map(fe).boxed()
                }),
            },
            Self::Compute { job } => {
                let job = Box::new(move || {
                    let command = job();
                    command.map(&fe, &fx)
                }) as Box<BlockingJob<E2, X2>>;
                Task::Compute { job }
            }
            Self::Blocking { job } => {
                let job = Box::new(move || {
                    let command = job();
                    command.map(&fe, &fx)
                }) as Box<BlockingJob<E2, X2>>;
                Task::Blocking { job }
            }
        }
    }

    /// Transform only the event output type.
    #[must_use]
    pub fn map_event<E2>(self, f: impl Fn(E) -> E2 + Send + 'static) -> Task<E2, X>
    where
        E2: Send + 'static,
    {
        self.map(f, std::convert::identity)
    }

    /// Transform only the effect output type.
    #[must_use]
    pub fn map_effect<X2>(self, f: impl Fn(X) -> X2 + Send + 'static) -> Task<E, X2>
    where
        X2: Send + 'static,
    {
        self.map(std::convert::identity, f)
    }
}

fn is_cancelled(cancel_guard: Option<&CancelGuard>) -> bool {
    cancel_guard.is_some_and(|guard| guard())
}

fn route_command<E, X>(
    event_tx: &EventSender<E>,
    effect_tx: &EffectSender<CommandStep<E, X>>,
    command: Command<E, X>,
    stats: Option<&ShellStats>,
    cancel_generations: Option<&CancelGenerationMap>,
    cancel_guard: Option<&CancelGuard>,
) where
    E: Send + 'static,
    X: Send + 'static,
{
    if is_cancelled(cancel_guard) {
        return;
    }

    for step in command {
        if is_cancelled(cancel_guard) {
            break;
        }

        match step {
            CommandStep::Event(event) => {
                if event_tx.send(event).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_event();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("event channel closed while routing command output");
                }
            }
            step => {
                let step = prepare_effect_step_for_queue(step, cancel_generations);
                if effect_tx.send(step).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_effect_step();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("effect channel closed while routing command output");
                }
            }
        }
    }
}

fn spawn_async_task<E, X>(
    async_executor: &Arc<dyn AsyncExecutor>,
    future: BoxFuture<'static, Command<E, X>>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    routing: TaskRouting<E, X>,
    panic_kind: PanicTaskKind,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    let activity_on_spawn_fail = routing.activity.clone();
    let TaskRouting {
        activity,
        stats,
        cancel_generations,
        cancel_guard,
        panic_handler,
    } = routing;

    if let Some(activity) = activity.as_ref() {
        activity.inc();
    }

    let fut = async move {
        let details = PanicDetails::new(panic_kind);
        let result = AssertUnwindSafe(future).catch_unwind().await;
        let command = match result {
            Ok(command) => command,
            Err(payload) => command_from_panic(panic_handler.as_ref(), details, payload),
        };
        route_command(
            &event_tx,
            &effect_tx,
            command,
            stats.as_ref(),
            cancel_generations.as_ref(),
            cancel_guard.as_ref(),
        );
        if let Some(activity) = activity {
            activity.dec();
        }
    }
    .boxed();

    if let Err(err) = async_executor.spawn_async(fut) {
        if let Some(activity) = activity_on_spawn_fail.as_ref() {
            activity.dec();
        }
        return Err(ShellError::TaskSpawnFailed(format!(
            "async executor: {err}"
        )));
    }

    Ok(())
}

fn spawn_stream_task<E, X>(
    async_executor: &Arc<dyn AsyncExecutor>,
    stream: BoxStream<'static, E>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    routing: TaskRouting<E, X>,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    let activity_on_spawn_fail = routing.activity.clone();
    let TaskRouting {
        activity,
        stats,
        cancel_generations,
        cancel_guard,
        panic_handler,
    } = routing;

    if let Some(activity) = activity.as_ref() {
        activity.inc();
    }

    let stream_event_tx = event_tx.clone();
    let stream_stats = stats.clone();
    let stream_cancel_guard = cancel_guard.clone();

    let fut = async move {
        let details = PanicDetails::new(PanicTaskKind::Stream);
        let result = AssertUnwindSafe(async move {
            futures_util::pin_mut!(stream);
            while let Some(event) = stream.next().await {
                if is_cancelled(stream_cancel_guard.as_ref()) {
                    break;
                }
                if stream_event_tx.send(event).is_err() {
                    if let Some(stats) = stream_stats.as_ref() {
                        stats.inc_dropped_event();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("event channel closed while forwarding stream item");
                    break;
                }
            }
        })
        .catch_unwind()
        .await;

        if let Err(payload) = result {
            let command = command_from_panic(panic_handler.as_ref(), details, payload);
            route_command(
                &event_tx,
                &effect_tx,
                command,
                stats.as_ref(),
                cancel_generations.as_ref(),
                cancel_guard.as_ref(),
            );
        }

        if let Some(activity) = activity {
            activity.dec();
        }
    }
    .boxed();

    if let Err(err) = async_executor.spawn_async(fut) {
        if let Some(activity) = activity_on_spawn_fail.as_ref() {
            activity.dec();
        }
        return Err(ShellError::TaskSpawnFailed(format!(
            "async stream executor: {err}"
        )));
    }

    Ok(())
}

fn spawn_blocking_task<E, X>(
    executor: &Arc<dyn BlockingExecutor>,
    job: Box<BlockingJob<E, X>>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    routing: TaskRouting<E, X>,
    panic_kind: PanicTaskKind,
    label: &'static str,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    let activity_on_spawn_fail = routing.activity.clone();
    let TaskRouting {
        activity,
        stats,
        cancel_generations,
        cancel_guard,
        panic_handler,
    } = routing;

    if let Some(activity) = activity.as_ref() {
        activity.inc();
    }

    let blocking_job = Box::new(move || {
        let details = PanicDetails::new(panic_kind);
        let command = match panic::catch_unwind(AssertUnwindSafe(job)) {
            Ok(command) => command,
            Err(payload) => command_from_panic(panic_handler.as_ref(), details, payload),
        };
        route_command(
            &event_tx,
            &effect_tx,
            command,
            stats.as_ref(),
            cancel_generations.as_ref(),
            cancel_guard.as_ref(),
        );
        if let Some(activity) = activity {
            activity.dec();
        }
    }) as Box<dyn FnOnce() + Send>;

    if let Err(err) = executor.spawn_blocking(blocking_job) {
        if let Some(activity) = activity_on_spawn_fail.as_ref() {
            activity.dec();
        }
        return Err(ShellError::TaskSpawnFailed(format!(
            "{label} executor: {err}"
        )));
    }

    Ok(())
}

#[allow(dead_code)]
pub(crate) fn drive_task<E, X>(
    async_executor: &Arc<dyn AsyncExecutor>,
    task: Task<E, X>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    panic_handler: Option<&Arc<PanicHook<E, X>>>,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    drive_task_with_activity(
        async_executor,
        None,
        None,
        task,
        event_tx,
        effect_tx,
        TaskRouting {
            panic_handler: panic_handler.cloned(),
            ..TaskRouting::default()
        },
    )
}

pub(crate) fn drive_task_with_activity<E, X>(
    async_executor: &Arc<dyn AsyncExecutor>,
    compute_executor: Option<&Arc<dyn BlockingExecutor>>,
    blocking_executor: Option<&Arc<dyn BlockingExecutor>>,
    task: Task<E, X>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    routing: TaskRouting<E, X>,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    match task {
        Task::None => Ok(()),
        Task::Resolved(command) => {
            route_command(
                &event_tx,
                &effect_tx,
                command,
                routing.stats.as_ref(),
                routing.cancel_generations.as_ref(),
                routing.cancel_guard.as_ref(),
            );
            Ok(())
        }
        Task::Async { factory } => {
            let runtime = AsyncRt::from_executor(Arc::clone(async_executor));
            let future = factory(runtime);
            spawn_async_task(
                async_executor,
                future,
                event_tx,
                effect_tx,
                routing,
                PanicTaskKind::Async,
            )
        }
        Task::Compute { job } => {
            let executor = compute_executor.or(blocking_executor);
            let executor = executor.ok_or_else(|| missing_executor("compute or blocking"))?;
            spawn_blocking_task(
                executor,
                job,
                event_tx,
                effect_tx,
                routing,
                PanicTaskKind::Compute,
                "compute",
            )
        }
        Task::Blocking { job } => {
            let executor = blocking_executor.or(compute_executor);
            let executor = executor.ok_or_else(|| missing_executor("blocking or compute"))?;
            spawn_blocking_task(
                executor,
                job,
                event_tx,
                effect_tx,
                routing,
                PanicTaskKind::Blocking,
                "blocking",
            )
        }
        Task::Stream { factory } => {
            let runtime = AsyncRt::from_executor(Arc::clone(async_executor));
            let stream = factory(runtime);
            spawn_stream_task(async_executor, stream, event_tx, effect_tx, routing)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Task;

    #[test]
    fn delayed_task_produces_correct_variant() {
        let task = Task::<(), ()>::delayed(Duration::from_millis(5), |_rt| async {
            super::Command::none()
        });

        assert!(matches!(task, Task::Async { .. }));
    }

    #[test]
    fn delayed_send_produces_async_variant() {
        let task = Task::<(), ()>::delayed_send(Duration::from_millis(5), ());

        assert!(matches!(task, Task::Async { .. }));
    }
}
