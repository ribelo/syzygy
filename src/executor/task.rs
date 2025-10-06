use std::any::{Any, TypeId};
use std::future::Future;

use crossbeam_channel::Sender;
use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{BoxStream, StreamExt};
// no-op import; keep tracing optional

use super::{
    AsyncOwnedExecutor, ErasedAsyncOwnedFn, ErasedOwnedSyncFn, ErasedSyncBorrowedFn,
    SyncBorrowedExecutor, SyncOwnedExecutor,
};
use crate::command::{Command, CommandStep};
use crate::error::ShellError;

fn missing_executor(kind: &'static str, exec: TypeId) -> ShellError {
    #[cfg(feature = "tracing")]
    tracing::error!(
        ?exec,
        kind,
        "Missing executor; effect could not be scheduled"
    );

    #[cfg(not(feature = "tracing"))]
    eprintln!("Missing {kind} executor for type {exec:?}; effect could not be scheduled");

    ShellError::TaskSpawnFailed(format!("Missing {kind} executor for type {exec:?}"))
}

type AsyncOwnedFutureFactory<E, X> =
    Box<dyn FnOnce(Box<dyn Any + Send>) -> BoxFuture<'static, Command<E, X>> + Send>;
type AsyncOwnedStreamFactory<E> = Box<dyn FnOnce(Box<dyn Any + Send>) -> BoxStream<'static, E> + Send>;
type SyncBorrowedFactory<E, X> = Box<dyn FnOnce(&mut dyn Any) -> Command<E, X> + Send>;
type SyncOwnedFactory<E, X> =
    Box<dyn FnOnce(Box<dyn Any + Send>) -> Command<E, X> + Send>;

pub enum Task<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    Event(E),
    Events(Vec<E>),
    AsyncOwned {
        exec_type_id: TypeId,
        resource_type_id: TypeId,
        make_future: AsyncOwnedFutureFactory<E, X>,
    },
    AsyncOwnedStream {
        exec_type_id: TypeId,
        resource_type_id: TypeId,
        make_stream: AsyncOwnedStreamFactory<E>,
    },
    SyncBorrowed {
        exec_type_id: TypeId,
        resource_type_id: TypeId,
        run_with_borrowed: SyncBorrowedFactory<E, X>,
    },
    SyncOwned {
        exec_type_id: TypeId,
        resource_type_id: TypeId,
        run_with_owned: SyncOwnedFactory<E, X>,
    },
}

struct AsyncOwnedJob<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    resource_type_id: TypeId,
    kind: AsyncOwnedKind<E, X>,
}
enum AsyncOwnedKind<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    Future(AsyncOwnedFutureFactory<E, X>),
    // Streams remain event-only (return events)
    Stream(AsyncOwnedStreamFactory<E>),
}

impl<E, X> ErasedAsyncOwnedFn<E, X> for AsyncOwnedJob<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    fn resource_type_id(&self) -> TypeId {
        self.resource_type_id
    }
    fn call(
        self: Box<Self>,
        state: Box<dyn Any + Send>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> BoxFuture<'static, ()> {
        let Self {
            resource_type_id: _,
            kind,
        } = *self;
        match kind {
            AsyncOwnedKind::Future(factory) => {
                let fut = factory(state);
                async move {
                    let command = fut.await;
                    route_command(&event_tx, &effect_tx, command);
                }
                .boxed()
            }
            AsyncOwnedKind::Stream(factory) => {
                let stream = factory(state);
                async move {
                    futures::pin_mut!(stream);
                    while let Some(event) = stream.next().await {
                        if let Err(_e) = event_tx.send(event) {
                            #[cfg(feature = "tracing")]
                            tracing::debug!("event channel closed while forwarding stream item");
                            break;
                        }
                    }
                }
                .boxed()
            }
        }
    }
}

struct SyncBorrowedJob<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    resource_type_id: TypeId,
    run_with_borrowed: SyncBorrowedFactory<E, X>,
}

impl<E, X> ErasedSyncBorrowedFn<E, X> for SyncBorrowedJob<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    fn resource_type_id(&self) -> TypeId {
        self.resource_type_id
    }

    fn call(
        self: Box<Self>,
        state: &mut dyn Any,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) {
        let Self {
            resource_type_id: _,
            run_with_borrowed,
        } = *self;
        let command = run_with_borrowed(state);
        route_command(&event_tx, &effect_tx, command);
    }
}

struct SyncOwnedJob<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    resource_type_id: TypeId,
    run_with_owned: SyncOwnedFactory<E, X>,
}

impl<E, X> ErasedOwnedSyncFn<E, X> for SyncOwnedJob<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    fn resource_type_id(&self) -> TypeId {
        self.resource_type_id
    }

    fn call(
        self: Box<Self>,
        state: Box<dyn Any + Send>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) {
        let Self {
            resource_type_id: _,
            run_with_owned,
        } = *self;
        let command = run_with_owned(state);
        route_command(&event_tx, &effect_tx, command);
    }
}

impl<E, X> Task<E, X>
where
    E: Send + Sync + 'static,
    X: Send + 'static,
{
    pub fn events<I>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
    {
        Self::Events(events.into_iter().collect())
    }

    #[must_use]
    pub fn none() -> Self {
        Self::Events(vec![])
    }

    pub fn event(event: E) -> Self {
        Self::Events(vec![event])
    }

    pub fn async_owned<Exec, F, Fut>(f: F) -> Self
    where
        Exec: AsyncOwnedExecutor<E> + 'static,
        Exec::Resources: Any + Send + 'static,
        F: FnOnce(Exec::Resources) -> Fut + Send + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let resource_type_id = TypeId::of::<Exec::Resources>();
        let factory: AsyncOwnedFutureFactory<E, X> = Box::new(move |resources| {
            // Safety: The resources match Exec::Resources; ensured by the executor lookup.
            unsafe {
                let resources = *resources.downcast_unchecked::<Exec::Resources>();
                Box::pin(f(resources).boxed())
            }
        });
        Self::AsyncOwned {
            exec_type_id,
            resource_type_id,
            make_future: factory,
        }
    }

    

    pub fn stream_owned<Exec, F>(f: F) -> Self
    where
        Exec: AsyncOwnedExecutor<E> + 'static,
        Exec::Resources: Any + Send + 'static,
        F: FnOnce(Exec::Resources) -> BoxStream<'static, E> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let resource_type_id = TypeId::of::<Exec::Resources>();
        let factory: AsyncOwnedStreamFactory<E> = Box::new(move |resources| {
            // Safety: resource type is validated via executor lookup.
            unsafe {
                let resources = *resources.downcast_unchecked::<Exec::Resources>();
                f(resources)
            }
        });
        Self::AsyncOwnedStream {
            exec_type_id,
            resource_type_id,
            make_stream: factory,
        }
    }

    

    pub fn sync_borrowed<Exec, F>(f: F) -> Self
    where
        Exec: SyncBorrowedExecutor<E> + 'static,
        Exec::Resources: Any + Send + 'static,
        F: FnOnce(&mut Exec::Resources) -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let resource_type_id = TypeId::of::<Exec::Resources>();
        let factory: SyncBorrowedFactory<E, X> = Box::new(move |resources| {
            // Safety: resource type is validated via executor lookup.
            unsafe {
                let resources = resources.downcast_mut_unchecked::<Exec::Resources>();
                f(resources)
            }
        });
        Self::SyncBorrowed {
            exec_type_id,
            resource_type_id,
            run_with_borrowed: factory,
        }
    }

    

    /// Create a sync task for an executor that supplies owned resources per job
    pub fn sync_owned<Exec, F>(f: F) -> Self
    where
        Exec: SyncOwnedExecutor<E> + 'static,
        Exec::Resources: Any + Send + 'static,
        F: FnOnce(Exec::Resources) -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let resource_type_id = TypeId::of::<Exec::Resources>();
        let factory: SyncOwnedFactory<E, X> = Box::new(move |resources| {
            // Safety: resource type validated via executor lookup.
            unsafe {
                let resources = *resources.downcast_unchecked::<Exec::Resources>();
                f(resources)
            }
        });
        Self::SyncOwned {
            exec_type_id,
            resource_type_id,
            run_with_owned: factory,
        }
    }

    
}

fn route_command<E, X>(
    event_tx: &crossbeam_channel::Sender<E>,
    effect_tx: &crossbeam_channel::Sender<CommandStep<E, X>>,
    command: Command<E, X>,
) where
    E: Send + Sync + 'static,
    X: Send + 'static,
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

use crate::executor::ExecutorRegistry;
use std::sync::Arc;

pub(crate) fn drive_spec<E, X>(
    executors: &Arc<ExecutorRegistry<E, X>>,
    spec: Task<E, X>,
    event_tx: crossbeam_channel::Sender<E>,
    effect_tx: crossbeam_channel::Sender<CommandStep<E, X>>,
) -> Result<(), ShellError>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    match spec {
        Task::Event(event) => {
            let _ = event_tx.send(event);
        }
        Task::Events(events) => {
            for e in events {
                let _ = event_tx.send(e);
            }
        }
        Task::AsyncOwned {
            exec_type_id,
            resource_type_id,
            make_future,
        } => {
            let job = Box::new(AsyncOwnedJob {
                resource_type_id,
                kind: AsyncOwnedKind::Future(make_future),
            }) as Box<dyn ErasedAsyncOwnedFn<E, X>>;
            dispatch_async_owned(
                "async",
                exec_type_id,
                resource_type_id,
                job,
                executors,
                event_tx,
                effect_tx,
            )?;
        }
        Task::AsyncOwnedStream {
            exec_type_id,
            resource_type_id,
            make_stream,
        } => {
            let job = Box::new(AsyncOwnedJob {
                resource_type_id,
                kind: AsyncOwnedKind::Stream(make_stream),
            }) as Box<dyn ErasedAsyncOwnedFn<E, X>>;
            dispatch_async_owned(
                "async-stream",
                exec_type_id,
                resource_type_id,
                job,
                executors,
                event_tx,
                effect_tx,
            )?;
        }
        Task::SyncBorrowed {
            exec_type_id,
            resource_type_id,
            run_with_borrowed,
        } => {
            let job = Box::new(SyncBorrowedJob {
                resource_type_id,
                run_with_borrowed,
            }) as Box<dyn ErasedSyncBorrowedFn<E, X>>;
            dispatch_sync_borrowed(executors, exec_type_id, resource_type_id, job, event_tx, effect_tx)?;
        }
        Task::SyncOwned {
            exec_type_id,
            resource_type_id,
            run_with_owned,
        } => {
            let job = Box::new(SyncOwnedJob {
                resource_type_id,
                run_with_owned,
            }) as Box<dyn ErasedOwnedSyncFn<E, X>>;
            dispatch_sync_owned(executors, exec_type_id, resource_type_id, job, event_tx, effect_tx)?;
        }
    }
    Ok(())
}

fn dispatch_async_owned<E, X>(
    kind: &'static str,
    exec_type_id: TypeId,
    resource_type_id: TypeId,
    job: Box<dyn ErasedAsyncOwnedFn<E, X>>,
    executors: &Arc<ExecutorRegistry<E, X>>,
    event_tx: Sender<E>,
    effect_tx: Sender<CommandStep<E, X>>,
) -> Result<(), ShellError>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    let exec_ref = executors
        .async_exec_by_key(exec_type_id)
        .ok_or_else(|| missing_executor(kind, exec_type_id))?;
    debug_assert_eq!(
        exec_ref.resource_type_id(),
        resource_type_id,
        "async executor state mismatch for {exec_type_id:?}"
    );
    exec_ref
        .spawn_async_owned_erased(job, event_tx, effect_tx)
        .map_err(|err| {
            ShellError::TaskSpawnFailed(format!("{kind} executor {exec_type_id:?}: {err}"))
        })
}

fn dispatch_sync_borrowed<E, X>(
    executors: &Arc<ExecutorRegistry<E, X>>,
    exec_type_id: TypeId,
    resource_type_id: TypeId,
    job: Box<dyn ErasedSyncBorrowedFn<E, X>>,
    event_tx: Sender<E>,
    effect_tx: Sender<CommandStep<E, X>>,
) -> Result<(), ShellError>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    let exec_ref = executors
        .sync_borrowed_exec_by_key(exec_type_id)
        .ok_or_else(|| missing_executor("sync", exec_type_id))?;
    debug_assert_eq!(
        exec_ref.resource_type_id(),
        resource_type_id,
        "sync executor state mismatch for {exec_type_id:?}"
    );
    exec_ref
        .spawn_sync_borrowed_erased(job, event_tx, effect_tx)
        .map_err(|err| {
            ShellError::TaskSpawnFailed(format!("sync executor {exec_type_id:?}: {err}"))
        })
}

fn dispatch_sync_owned<E, X>(
    executors: &Arc<ExecutorRegistry<E, X>>,
    exec_type_id: TypeId,
    resource_type_id: TypeId,
    job: Box<dyn ErasedOwnedSyncFn<E, X>>,
    event_tx: Sender<E>,
    effect_tx: Sender<CommandStep<E, X>>,
) -> Result<(), ShellError>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    let exec_ref = executors
        .sync_owned_exec_by_key(exec_type_id)
        .ok_or_else(|| missing_executor("sync-owned", exec_type_id))?;
    debug_assert_eq!(
        exec_ref.resource_type_id(),
        resource_type_id,
        "sync-owned executor state mismatch for {exec_type_id:?}"
    );
    exec_ref
        .spawn_sync_owned_erased(job, event_tx, effect_tx)
        .map_err(|err| {
            ShellError::TaskSpawnFailed(format!("sync-owned executor {exec_type_id:?}: {err}"))
        })
}
