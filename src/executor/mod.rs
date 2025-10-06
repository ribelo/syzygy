//! Executors — specialized async and sync runtimes.

use std::any::{Any, TypeId};
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;
use futures_util::future::BoxFuture;
use thiserror::Error;

pub mod inline_async;
#[cfg(feature = "rayon")]
pub mod rayon_sync_executor;
pub mod registry;
pub mod single_thread_executor;
pub mod task;
#[cfg(feature = "tokio")]
pub mod tokio_executor;

pub use inline_async::InlineAsync;
#[cfg(feature = "rayon")]
pub use rayon_sync_executor::{RayonExecutor, RayonExecutorBuilder};
pub use registry::ExecutorRegistry;
pub use single_thread_executor::SingleThreadExecutor;
pub use task::Task;
#[cfg(feature = "tokio")]
pub use tokio_executor::{TokioExecutor, TokioExecutorBuilder};

/// Error type returned when executors fail to schedule jobs.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("executor has been shut down")]
    Shutdown,
    #[error("executor worker is gone")]
    WorkerGone,
    #[error("task was cancelled")]
    Cancelled,
    #[error("panic: {msg}")]
    Panic { msg: String },
}

#[must_use]
pub fn panic_message(panic_payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic_payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = panic_payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unknown internal error".to_string()
    }
}

/// Shared lifecycle management for all executor types.
pub trait ExecutorLifecycle: Send + Sync + 'static {
    fn shutdown(&self);
    fn join(&self) -> BoxFuture<'static, ()>;
}

/// Executor specialized for async work (futures).
///
/// Jobs receive owned executor resources and must return a future that forwards
/// any produced events to Core before completing.
/// Async executor that schedules background tasks with owned resources.
///
/// This is the primary async contract used by executors like Tokio: tasks run
/// in the background and therefore must be `'static`. Jobs receive resources
/// by value (owned/cloneable) and return a future that completes independently
/// of the caller.
pub trait AsyncOwnedExecutor<E>: ExecutorLifecycle {
    type Resources: Clone + Send + Sync + 'static;

    fn spawn_owned<F, Fut>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(Self::Resources) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static;

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// Executor specialized for blocking/synchronous work.
///
/// Jobs receive mutable access to executor resources and must forward events to
/// Core before returning.
pub trait SyncBorrowedExecutor<E>: ExecutorLifecycle {
    type Resources: Send + Sync + 'static;

    fn spawn_sync<F>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(&mut Self::Resources) + Send + 'static;
}

/// Executor specialized for blocking/synchronous work with owned resources per job.
///
/// Jobs receive owned executor resources (cloned or otherwise produced per job)
/// and must forward events to Core before returning.
pub trait SyncOwnedExecutor<E>: ExecutorLifecycle {
    type Resources: Send + Sync + 'static;

    fn spawn_sync_owned<F>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(Self::Resources) + Send + 'static;
}

/// Type-erased async executor used by the registry.
use crate::command::CommandStep;

pub trait DynAsyncExecutor<E, X>: ExecutorLifecycle {
    fn executor_type_id(&self) -> TypeId;
    fn resource_type_id(&self) -> TypeId;

    fn spawn_async_owned_erased(
        &self,
        job: Box<dyn ErasedAsyncOwnedFn<E, X>>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> Result<(), ExecutorError>;

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// Type-erased sync executor used by the registry.
pub trait DynSyncBorrowedExecutor<E, X>: ExecutorLifecycle {
    fn executor_type_id(&self) -> TypeId;
    fn resource_type_id(&self) -> TypeId;

    fn spawn_sync_borrowed_erased(
        &self,
        job: Box<dyn ErasedSyncBorrowedFn<E, X>>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> Result<(), ExecutorError>;
}

/// Type-erased owned sync executor used by the registry.
pub trait DynSyncOwnedExecutor<E, X>: ExecutorLifecycle {
    fn executor_type_id(&self) -> TypeId;
    fn resource_type_id(&self) -> TypeId;

    fn spawn_sync_owned_erased(
        &self,
        job: Box<dyn ErasedOwnedSyncFn<E, X>>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> Result<(), ExecutorError>;
}

/// Type-erased async job.
pub trait ErasedAsyncOwnedFn<E, X>: Send {
    fn resource_type_id(&self) -> TypeId;
    fn call(
        self: Box<Self>,
        resources: Box<dyn Any + Send>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> BoxFuture<'static, ()>;
}

/// Type-erased sync job.
pub trait ErasedSyncBorrowedFn<E, X>: Send {
    fn resource_type_id(&self) -> TypeId;
    fn call(
        self: Box<Self>,
        resources: &mut dyn Any,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    );
}

/// Type-erased owned sync job.
pub trait ErasedOwnedSyncFn<E, X>: Send {
    fn resource_type_id(&self) -> TypeId;
    fn call(
        self: Box<Self>,
        resources: Box<dyn Any + Send>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    );
}

/// Adapter turning a typed async executor into a type-erased executor.
pub struct DynAsyncExecutorAdapter<E, X, T>
where
    T: AsyncOwnedExecutor<E>,
{
    inner: Arc<T>,
    _marker: PhantomData<(E, X)>,
}

impl<E, X, T> DynAsyncExecutorAdapter<E, X, T>
where
    T: AsyncOwnedExecutor<E>,
{
    pub fn new(inner: Arc<T>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<E, X, T> ExecutorLifecycle for DynAsyncExecutorAdapter<E, X, T>
where
    E: Send + Sync + 'static,
    T: AsyncOwnedExecutor<E> + 'static,
    X: Send + Sync + 'static,
{
    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        self.inner.join()
    }
}

impl<E, X, T> DynAsyncExecutor<E, X> for DynAsyncExecutorAdapter<E, X, T>
where
    E: Send + Sync + 'static,
    T: AsyncOwnedExecutor<E> + 'static,
    T::Resources: Any + Send + Sync,
    X: Send + Sync + 'static,
{
    fn executor_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn resource_type_id(&self) -> TypeId {
        TypeId::of::<T::Resources>()
    }

    fn spawn_async_owned_erased(
        &self,
        job: Box<dyn ErasedAsyncOwnedFn<E, X>>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> Result<(), ExecutorError> {
        let mut maybe_job = Some(job);
        self.inner.spawn_owned(move |resources: T::Resources| {
            let job = maybe_job.take().expect("async job already taken");
            job.call(Box::new(resources), event_tx.clone(), effect_tx.clone())
        })
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        self.inner.sleep(duration)
    }
}

/// Adapter turning a typed sync executor into a type-erased executor.
pub struct DynSyncBorrowedExecutorAdapter<E, X, T>
where
    T: SyncBorrowedExecutor<E>,
{
    inner: Arc<T>,
    _marker: PhantomData<(E, X)>,
}

impl<E, X, T> DynSyncBorrowedExecutorAdapter<E, X, T>
where
    T: SyncBorrowedExecutor<E>,
{
    pub fn new(inner: Arc<T>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<E, X, T> ExecutorLifecycle for DynSyncBorrowedExecutorAdapter<E, X, T>
where
    E: Send + Sync + 'static,
    T: SyncBorrowedExecutor<E> + 'static,
    X: Send + Sync + 'static,
{
    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        self.inner.join()
    }
}

impl<E, X, T> DynSyncBorrowedExecutor<E, X> for DynSyncBorrowedExecutorAdapter<E, X, T>
where
    E: Send + Sync + 'static,
    T: SyncBorrowedExecutor<E> + 'static,
    T::Resources: Any + Send + Sync,
    X: Send + Sync + 'static,
{
    fn executor_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn resource_type_id(&self) -> TypeId {
        TypeId::of::<T::Resources>()
    }

    fn spawn_sync_borrowed_erased(
        &self,
        job: Box<dyn ErasedSyncBorrowedFn<E, X>>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> Result<(), ExecutorError> {
        let mut maybe_job = Some(job);
        self.inner.spawn_sync(move |resources: &mut T::Resources| {
            let job = maybe_job.take().expect("sync job already taken");
            job.call(
                resources as &mut dyn Any,
                event_tx.clone(),
                effect_tx.clone(),
            );
        })
    }
}

/// Adapter turning a typed owned-sync executor into a type-erased executor.
pub struct DynSyncOwnedExecutorAdapter<E, X, T>
where
    T: SyncOwnedExecutor<E>,
{
    inner: Arc<T>,
    _marker: PhantomData<(E, X)>,
}

impl<E, X, T> DynSyncOwnedExecutorAdapter<E, X, T>
where
    T: SyncOwnedExecutor<E>,
{
    pub fn new(inner: Arc<T>) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<E, X, T> ExecutorLifecycle for DynSyncOwnedExecutorAdapter<E, X, T>
where
    E: Send + Sync + 'static,
    T: SyncOwnedExecutor<E> + 'static,
    X: Send + Sync + 'static,
{
    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        self.inner.join()
    }
}

impl<E, X, T> DynSyncOwnedExecutor<E, X> for DynSyncOwnedExecutorAdapter<E, X, T>
where
    E: Send + Sync + 'static,
    T: SyncOwnedExecutor<E> + 'static,
    T::Resources: Any + Send + Sync,
    X: Send + Sync + 'static,
{
    fn executor_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn resource_type_id(&self) -> TypeId {
        TypeId::of::<T::Resources>()
    }

    fn spawn_sync_owned_erased(
        &self,
        job: Box<dyn ErasedOwnedSyncFn<E, X>>,
        event_tx: Sender<E>,
        effect_tx: Sender<CommandStep<E, X>>,
    ) -> Result<(), ExecutorError> {
        let mut maybe_job = Some(job);
        self.inner.spawn_sync_owned(move |resources: T::Resources| {
            let job = maybe_job.take().expect("owned sync job already taken");
            job.call(Box::new(resources), event_tx.clone(), effect_tx.clone());
        })
    }
}
