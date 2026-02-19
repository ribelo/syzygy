//! Declarative task plans returned by effect handlers.
//!
//! A `Task<E, X>` describes what work to schedule and on which executor type.
//! The Shell interprets the plan and routes any resulting `Command` steps back
//! into the Core/Shell pipeline.
use std::any::{type_name, Any, TypeId};
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use crossbeam_channel::Sender as EffectSender;
use futures_util::future::{BoxFuture, Either, FutureExt};
use futures_util::stream::{BoxStream, StreamExt};

use crate::activity::Activity;
use crate::command::{Command, CommandStep};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::{
    panic_message, AsyncExecutor, BlockingExecutor, ExecutorRegistry, ResourceBlockingExecutor,
};
use crate::shell::{prepare_effect_step_for_queue, CancelGenerationMap, CancelGuard, ShellStats};

fn missing_executor(kind: &'static str, exec: TypeId, exec_name: &'static str) -> ShellError {
    #[cfg(feature = "tracing")]
    tracing::error!(
        ?exec,
        kind,
        exec_name,
        "Missing executor; effect could not be scheduled"
    );

    ShellError::TaskSpawnFailed(format!(
        "Missing {kind} executor: {exec_name} (TypeId={exec:?})"
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicTaskKind {
    Async,
    Blocking,
    BlockingWithResource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanicDetails {
    pub kind: PanicTaskKind,
    pub executor_type_name: &'static str,
    pub executor_type_id: TypeId,
}

impl PanicDetails {
    #[must_use]
    pub fn new(
        kind: PanicTaskKind,
        executor_type_name: &'static str,
        executor_type_id: TypeId,
    ) -> Self {
        Self {
            kind,
            executor_type_name,
            executor_type_id,
        }
    }
}

pub type PanicHook<E, X> = dyn Fn(PanicDetails, String) -> Command<E, X> + Send + Sync;
type BlockingJob<E, X> = dyn FnOnce() -> Command<E, X> + Send;
type ResourceBlockingJob<E, X> = dyn FnOnce(&mut dyn Any) -> Command<E, X> + Send;

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
    Event(E),
    Events(Vec<E>),
    Async {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        future: BoxFuture<'static, Command<E, X>>,
    },
    Stream {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        stream: BoxStream<'static, E>,
    },
    Blocking {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        job: Box<BlockingJob<E, X>>,
    },
    BlockingWithResource {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        resource_type_id: TypeId,
        resource_type_name: &'static str,
        job: Box<ResourceBlockingJob<E, X>>,
    },
}

impl<E, X> Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// Emit multiple events back to Core.
    pub fn events<I>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
    {
        Self::Events(events.into_iter().collect())
    }

    /// No-op task. Useful when effects are conditionally skipped.
    #[must_use]
    pub fn none() -> Self {
        Self::Events(vec![])
    }

    /// Emit a single event back to Core.
    pub fn event(event: E) -> Self {
        Self::Event(event)
    }

    /// Run a future on a specific async executor type.
    ///
    /// Selects the executor by its concrete type; register the same type on
    /// the builder. The future resolves to a `Command` whose outputs are routed
    /// back through the system.
    pub fn async_on<Exec, Fut>(future: Fut) -> Self
    where
        Exec: AsyncExecutor + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let future: BoxFuture<'static, Command<E, X>> = future.boxed();
        Self::Async {
            exec_type_id,
            exec_type_name,
            future,
        }
    }

    /// Run a future on a specific async executor with cancellation support.
    pub fn async_on_with_cancel<Exec, Fut, Cancel>(
        future: Fut,
        cancel: Cancel,
        cancel_command: Command<E, X>,
    ) -> Self
    where
        Exec: AsyncExecutor + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
        Cancel: Future<Output = ()> + Send + 'static,
    {
        let fut = async move {
            futures_util::pin_mut!(future);
            futures_util::pin_mut!(cancel);
            match futures_util::future::select(cancel, future).await {
                Either::Left(((), _pending_future)) => cancel_command,
                Either::Right((command, _)) => command,
            }
        };
        Self::async_on::<Exec, _>(fut)
    }

    /// Forward a stream’s items as events on a specific async executor.
    pub fn stream_on<Exec, S>(stream: S) -> Self
    where
        Exec: AsyncExecutor + 'static,
        S: futures_util::stream::Stream<Item = E> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let stream: BoxStream<'static, E> = stream.boxed();
        Self::Stream {
            exec_type_id,
            exec_type_name,
            stream,
        }
    }

    /// Run a blocking job on a blocking executor (no shared mutable resource).
    pub fn blocking_on<Exec, F>(job: F) -> Self
    where
        Exec: BlockingExecutor + 'static,
        F: FnOnce() -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let job: Box<BlockingJob<E, X>> = Box::new(job);
        Self::Blocking {
            exec_type_id,
            exec_type_name,
            job,
        }
    }

    /// Run a blocking job that requires mutable access to an executor-owned resource.
    pub fn blocking_with_resource_on<Exec, R, F>(job: F) -> Self
    where
        Exec: ResourceBlockingExecutor + 'static,
        R: 'static,
        F: FnOnce(&mut R) -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let resource_type_id = TypeId::of::<R>();
        let resource_type_name = type_name::<R>();
        let user_job = job;
        let job = Box::new(move |resource: &mut dyn Any| {
            let resource = resource.downcast_mut::<R>().unwrap_or_else(|| {
                panic!("resource type mismatch for single-thread executor; expected {resource_type_name}")
            });
            user_job(resource)
        }) as Box<ResourceBlockingJob<E, X>>;
        Self::BlockingWithResource {
            exec_type_id,
            exec_type_name,
            resource_type_id,
            resource_type_name,
            job,
        }
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
            Self::Event(event) => Task::Event(fe(event)),
            Self::Events(events) => Task::Events(events.into_iter().map(&fe).collect()),
            Self::Async {
                exec_type_id,
                exec_type_name,
                future,
            } => {
                let future = future.map(move |command| command.map(&fe, &fx)).boxed();
                Task::Async {
                    exec_type_id,
                    exec_type_name,
                    future,
                }
            }
            Self::Stream {
                exec_type_id,
                exec_type_name,
                stream,
            } => {
                let stream = stream.map(fe).boxed();
                Task::Stream {
                    exec_type_id,
                    exec_type_name,
                    stream,
                }
            }
            Self::Blocking {
                exec_type_id,
                exec_type_name,
                job,
            } => {
                let job = Box::new(move || {
                    let command = job();
                    command.map(&fe, &fx)
                }) as Box<BlockingJob<E2, X2>>;
                Task::Blocking {
                    exec_type_id,
                    exec_type_name,
                    job,
                }
            }
            Self::BlockingWithResource {
                exec_type_id,
                exec_type_name,
                resource_type_id,
                resource_type_name,
                job,
            } => {
                let job = Box::new(move |resource: &mut dyn Any| {
                    let command = job(resource);
                    command.map(&fe, &fx)
                }) as Box<ResourceBlockingJob<E2, X2>>;
                Task::BlockingWithResource {
                    exec_type_id,
                    exec_type_name,
                    resource_type_id,
                    resource_type_name,
                    job,
                }
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

#[allow(dead_code)]
pub(crate) fn drive_task<E, X>(
    executors: &Arc<ExecutorRegistry<E>>,
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
        executors,
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
    executors: &Arc<ExecutorRegistry<E>>,
    task: Task<E, X>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    routing: TaskRouting<E, X>,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    let TaskRouting {
        activity,
        stats,
        cancel_generations,
        cancel_guard,
        panic_handler,
    } = routing;
    match task {
        Task::Event(event) => {
            if !is_cancelled(cancel_guard.as_ref()) && event_tx.send(event).is_err() {
                if let Some(stats) = stats.as_ref() {
                    stats.inc_dropped_event();
                }
                #[cfg(feature = "tracing")]
                tracing::debug!("event channel closed while dispatching event task");
            }
        }
        Task::Events(events) => {
            for e in events {
                if is_cancelled(cancel_guard.as_ref()) {
                    break;
                }

                if event_tx.send(e).is_err() {
                    if let Some(stats) = stats.as_ref() {
                        stats.inc_dropped_event();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("event channel closed while dispatching event task");
                    break;
                }
            }
        }
        Task::Async {
            exec_type_id,
            exec_type_name,
            future,
        } => {
            let exec = executors
                .async_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("async", exec_type_id, exec_type_name))?;

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let activity_cl = activity.clone();
            let stats_cl = stats.clone();
            let cancel_generations_cl = cancel_generations.clone();
            let cancel_guard_cl = cancel_guard.clone();
            if let Some(activity) = activity.as_ref() {
                activity.inc();
            }
            let panic_handler_cl = panic_handler.clone();
            let fut = async move {
                let details = PanicDetails::new(PanicTaskKind::Async, exec_type_name, exec_type_id);
                let result = AssertUnwindSafe(future).catch_unwind().await;
                let command = match result {
                    Ok(command) => command,
                    Err(payload) => command_from_panic(panic_handler_cl.as_ref(), details, payload),
                };
                let stats_ref = stats_cl.as_ref();
                let cancel_generations_ref = cancel_generations_cl.as_ref();
                let cancel_guard_ref = cancel_guard_cl.as_ref();
                route_command(
                    &event_tx_cl,
                    &effect_tx_cl,
                    command,
                    stats_ref,
                    cancel_generations_ref,
                    cancel_guard_ref,
                );
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }
            .boxed();

            if let Err(err) = exec.spawn_async(fut) {
                if let Some(activity) = activity.as_ref() {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "async executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
        Task::Stream {
            exec_type_id,
            exec_type_name,
            stream,
        } => {
            let exec = executors
                .async_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("stream", exec_type_id, exec_type_name))?;

            let event_tx_cl = event_tx.clone();
            let activity_cl = activity.clone();
            let stats_cl = stats.clone();
            let cancel_guard_cl = cancel_guard.clone();
            if let Some(activity) = activity.as_ref() {
                activity.inc();
            }
            let fut = async move {
                futures_util::pin_mut!(stream);
                while let Some(event) = stream.next().await {
                    if is_cancelled(cancel_guard_cl.as_ref()) {
                        break;
                    }

                    if event_tx_cl.send(event).is_err() {
                        if let Some(stats) = stats_cl.as_ref() {
                            stats.inc_dropped_event();
                        }
                        #[cfg(feature = "tracing")]
                        tracing::debug!("event channel closed while forwarding stream item");
                        break;
                    }
                }
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }
            .boxed();

            if let Err(err) = exec.spawn_async(fut) {
                if let Some(activity) = activity.as_ref() {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "stream executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
        Task::Blocking {
            exec_type_id,
            exec_type_name,
            job,
        } => {
            let exec = executors
                .blocking_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("blocking", exec_type_id, exec_type_name))?;

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let activity_cl = activity.clone();
            let stats_cl = stats.clone();
            let cancel_generations_cl = cancel_generations.clone();
            let cancel_guard_cl = cancel_guard.clone();
            if let Some(activity) = activity.as_ref() {
                activity.inc();
            }
            let panic_handler_cl = panic_handler.clone();
            let job = Box::new(move || {
                let details =
                    PanicDetails::new(PanicTaskKind::Blocking, exec_type_name, exec_type_id);
                let command = match panic::catch_unwind(AssertUnwindSafe(job)) {
                    Ok(command) => command,
                    Err(payload) => command_from_panic(panic_handler_cl.as_ref(), details, payload),
                };
                let stats_ref = stats_cl.as_ref();
                let cancel_generations_ref = cancel_generations_cl.as_ref();
                let cancel_guard_ref = cancel_guard_cl.as_ref();
                route_command(
                    &event_tx_cl,
                    &effect_tx_cl,
                    command,
                    stats_ref,
                    cancel_generations_ref,
                    cancel_guard_ref,
                );
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }) as Box<dyn FnOnce() + Send>;

            if let Err(err) = exec.spawn_blocking(job) {
                if let Some(activity) = activity.as_ref() {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "blocking executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
        Task::BlockingWithResource {
            exec_type_id,
            exec_type_name,
            resource_type_id,
            resource_type_name,
            job,
        } => {
            let exec = executors
                .resource_blocking_exec_by_key(exec_type_id)
                .ok_or_else(|| {
                    missing_executor("resource-blocking", exec_type_id, exec_type_name)
                })?;

            let actual = exec.resource_type_id();
            if actual != resource_type_id {
                return Err(ShellError::TaskSpawnFailed(format!(
                    "resource type mismatch for executor {exec_type_name} (TypeId={exec_type_id:?}): expected resource {resource_type_name} (TypeId={resource_type_id:?}), got TypeId={actual:?}"
                )));
            }

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let activity_cl = activity.clone();
            let stats_cl = stats.clone();
            let cancel_generations_cl = cancel_generations.clone();
            let cancel_guard_cl = cancel_guard.clone();
            if let Some(activity) = activity.as_ref() {
                activity.inc();
            }
            let panic_handler_cl = panic_handler.clone();
            let job = Box::new(move |resource: &mut dyn Any| {
                let details = PanicDetails::new(
                    PanicTaskKind::BlockingWithResource,
                    exec_type_name,
                    exec_type_id,
                );
                let command = match panic::catch_unwind(AssertUnwindSafe(|| job(resource))) {
                    Ok(command) => command,
                    Err(payload) => command_from_panic(panic_handler_cl.as_ref(), details, payload),
                };
                let stats_ref = stats_cl.as_ref();
                let cancel_generations_ref = cancel_generations_cl.as_ref();
                let cancel_guard_ref = cancel_guard_cl.as_ref();
                route_command(
                    &event_tx_cl,
                    &effect_tx_cl,
                    command,
                    stats_ref,
                    cancel_generations_ref,
                    cancel_guard_ref,
                );
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }) as Box<dyn FnOnce(&mut dyn Any) + Send>;

            if let Err(err) = exec.spawn_blocking_with_resource(job) {
                if let Some(activity) = activity.as_ref() {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "resource-blocking executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
    }

    Ok(())
}
