//! Declarative task plans returned by effect handlers.
//!
//! A `Task<E, X>` describes what work to schedule and on which executor type.
//! The Shell interprets the plan and routes any resulting `Command` steps back
//! into the Core/Shell pipeline.
use std::any::{Any, TypeId};
use std::future::Future;
use std::sync::Arc;

use crossbeam_channel::Sender;
#[cfg(feature = "tokio")]
use futures::executor::block_on;
use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{BoxStream, StreamExt};

use crate::command::{Command, CommandStep};
use crate::error::ShellError;
use crate::executor::{
    AsyncExecutor, BlockingExecutor, ExecutorRegistry, ResourceBlockingExecutor,
};

fn missing_executor(kind: &'static str, exec: TypeId) -> ShellError {
    #[cfg(feature = "tracing")]
    tracing::error!(
        ?exec,
        kind,
        "Missing executor; effect could not be scheduled"
    );

    ShellError::TaskSpawnFailed(format!("Missing {kind} executor for type {exec:?}"))
}

/// Declarative unit of work returned by effect handlers.
pub enum Task<E, X>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    Event(E),
    Events(Vec<E>),
    Async {
        exec_type_id: TypeId,
        future: BoxFuture<'static, Command<E, X>>,
    },
    Stream {
        exec_type_id: TypeId,
        stream: BoxStream<'static, E>,
    },
    #[cfg(feature = "tokio")]
    /// Run on the current async runtime if available (e.g. inside #[tokio::main]).
    /// Falls back to blocking execution on the current thread if no runtime.
    AsyncCurrent {
        future: BoxFuture<'static, Command<E, X>>,
    },
    #[cfg(feature = "tokio")]
    /// Forward a stream on the current async runtime if available.
    /// Falls back to draining the stream on the current thread if no runtime.
    StreamCurrent {
        stream: BoxStream<'static, E>,
    },
    Blocking {
        exec_type_id: TypeId,
        job: Box<dyn FnOnce() -> Command<E, X> + Send>,
    },
    BlockingWithResource {
        exec_type_id: TypeId,
        resource_type_id: TypeId,
        job: Box<dyn FnOnce(&mut dyn Any) -> Command<E, X> + Send>,
    },
}

impl<E, X> Task<E, X>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
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
        Exec: AsyncExecutor<E> + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let future: BoxFuture<'static, Command<E, X>> = future.boxed();
        Self::Async {
            exec_type_id,
            future,
        }
    }

    #[cfg(feature = "tokio")]
    /// Create an async task that runs on the current runtime if present,
    /// otherwise completes inline by blocking the current thread.
    pub fn async_current<Fut>(future: Fut) -> Self
    where
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        let future: BoxFuture<'static, Command<E, X>> = future.boxed();
        Self::AsyncCurrent { future }
    }

    #[cfg(feature = "tokio")]
    /// Create a stream task that runs on the current runtime if present,
    /// otherwise drains inline by blocking the current thread.
    pub fn stream_current<S>(stream: S) -> Self
    where
        S: futures_util::stream::Stream<Item = E> + Send + 'static,
    {
        let stream: BoxStream<'static, E> = stream.boxed();
        Self::StreamCurrent { stream }
    }

    /// Forward a stream’s items as events on a specific async executor.
    pub fn stream_on<Exec, S>(stream: S) -> Self
    where
        Exec: AsyncExecutor<E> + 'static,
        S: futures_util::stream::Stream<Item = E> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let stream: BoxStream<'static, E> = stream.boxed();
        Self::Stream {
            exec_type_id,
            stream,
        }
    }

    /// Run a blocking job on a blocking executor (no shared mutable resource).
    pub fn blocking_on<Exec, F>(job: F) -> Self
    where
        Exec: BlockingExecutor<E> + 'static,
        F: FnOnce() -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let job: Box<dyn FnOnce() -> Command<E, X> + Send> = Box::new(job);
        Self::Blocking { exec_type_id, job }
    }

    /// Run a blocking job that requires mutable access to an executor-owned resource.
    pub fn blocking_with_resource_on<Exec, R, F>(job: F) -> Self
    where
        Exec: ResourceBlockingExecutor<E> + 'static,
        R: 'static,
        F: FnOnce(&mut R) -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let resource_type_id = TypeId::of::<R>();
        let job = Box::new(move |resource: &mut dyn Any| {
            let resource = resource
                .downcast_mut::<R>()
                .expect("resource type mismatch for single-thread executor");
            job(resource)
        }) as Box<dyn FnOnce(&mut dyn Any) -> Command<E, X> + Send>;
        Self::BlockingWithResource {
            exec_type_id,
            resource_type_id,
            job,
        }
    }
}

#[cfg(feature = "tokio")]
impl<E, X> From<BoxFuture<'static, Command<E, X>>> for Task<E, X>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    fn from(future: BoxFuture<'static, Command<E, X>>) -> Self {
        Task::AsyncCurrent { future }
    }
}

#[cfg(feature = "tokio")]
impl<E, X> From<BoxStream<'static, E>> for Task<E, X>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    fn from(stream: BoxStream<'static, E>) -> Self {
        Task::StreamCurrent { stream }
    }
}

fn route_command<E, X>(
    event_tx: &Sender<E>,
    effect_tx: &Sender<CommandStep<E, X>>,
    command: Command<E, X>,
) where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    for step in command {
        match step {
            CommandStep::Event(event) => {
                let _ = event_tx.send(event);
            }
            CommandStep::Effect(x) => {
                let _ = effect_tx.send(CommandStep::Effect(x));
            }
            CommandStep::Batch(v) => {
                let _ = effect_tx.send(CommandStep::Batch(v));
            }
            CommandStep::Parallel(v) => {
                let _ = effect_tx.send(CommandStep::Parallel(v));
            }
        }
    }
}

pub(crate) fn drive_task<E, X>(
    executors: &Arc<ExecutorRegistry<E>>,
    task: Task<E, X>,
    event_tx: Sender<E>,
    effect_tx: Sender<CommandStep<E, X>>,
) -> Result<(), ShellError>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    match task {
        Task::Event(event) => {
            let _ = event_tx.send(event);
        }
        Task::Events(events) => {
            for e in events {
                let _ = event_tx.send(e);
            }
        }
        #[cfg(feature = "tokio")]
        Task::AsyncCurrent { future } => {
            // Try to spawn on the current Tokio runtime; if unavailable, run inline.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let event_tx_cl = event_tx.clone();
                let effect_tx_cl = effect_tx.clone();
                let fut = async move {
                    let command = future.await;
                    route_command(&event_tx_cl, &effect_tx_cl, command);
                };
                handle.spawn(fut);
            } else {
                let command = block_on(future);
                route_command(&event_tx, &effect_tx, command);
            }
        }
        Task::Async {
            exec_type_id,
            future,
        } => {
            let exec = executors
                .async_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("async", exec_type_id))?;

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let fut = async move {
                let command = future.await;
                route_command(&event_tx_cl, &effect_tx_cl, command);
            }
            .boxed();

            exec.spawn_async(fut).map_err(|err| {
                ShellError::TaskSpawnFailed(format!("async executor {exec_type_id:?}: {err}"))
            })?;
        }
        #[cfg(feature = "tokio")]
        Task::StreamCurrent { stream } => {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let event_tx_cl = event_tx.clone();
                let fut = async move {
                    futures_util::pin_mut!(stream);
                    while let Some(event) = stream.next().await {
                        if event_tx_cl.send(event).is_err() {
                            #[cfg(feature = "tracing")]
                            tracing::debug!("event channel closed while forwarding stream item");
                            break;
                        }
                    }
                };
                handle.spawn(fut);
            } else {
                block_on(async move {
                    futures_util::pin_mut!(stream);
                    while let Some(event) = stream.next().await {
                        if event_tx.send(event).is_err() {
                            #[cfg(feature = "tracing")]
                            tracing::debug!("event channel closed while forwarding stream item");
                            break;
                        }
                    }
                });
            }
        }
        Task::Stream {
            exec_type_id,
            stream,
        } => {
            let exec = executors
                .async_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("stream", exec_type_id))?;

            let event_tx_cl = event_tx.clone();
            let fut = async move {
                futures_util::pin_mut!(stream);
                while let Some(event) = stream.next().await {
                    if event_tx_cl.send(event).is_err() {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("event channel closed while forwarding stream item");
                        break;
                    }
                }
            }
            .boxed();

            exec.spawn_async(fut).map_err(|err| {
                ShellError::TaskSpawnFailed(format!("stream executor {exec_type_id:?}: {err}"))
            })?;
        }
        Task::Blocking { exec_type_id, job } => {
            let exec = executors
                .blocking_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("blocking", exec_type_id))?;

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let job = Box::new(move || {
                let command = job();
                route_command(&event_tx_cl, &effect_tx_cl, command);
            }) as Box<dyn FnOnce() + Send>;

            exec.spawn_blocking(job).map_err(|err| {
                ShellError::TaskSpawnFailed(format!("blocking executor {exec_type_id:?}: {err}"))
            })?;
        }
        Task::BlockingWithResource {
            exec_type_id,
            resource_type_id,
            job,
        } => {
            let exec = executors
                .resource_blocking_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("resource-blocking", exec_type_id))?;

            let actual = exec.resource_type_id();
            if actual != resource_type_id {
                return Err(ShellError::TaskSpawnFailed(format!(
                    "resource type mismatch for executor {exec_type_id:?}: expected {resource_type_id:?}, got {actual:?}"
                )));
            }

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let job = Box::new(move |resource: &mut dyn Any| {
                let command = job(resource);
                route_command(&event_tx_cl, &effect_tx_cl, command);
            }) as Box<dyn FnOnce(&mut dyn Any) + Send>;

            exec.spawn_blocking_with_resource(job).map_err(|err| {
                ShellError::TaskSpawnFailed(format!(
                    "resource-blocking executor {exec_type_id:?}: {err}"
                ))
            })?;
        }
    }

    Ok(())
}
