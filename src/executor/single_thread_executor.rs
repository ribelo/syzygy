use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};
use futures::channel::oneshot;
use futures::executor::block_on;
use futures_util::future::{BoxFuture, FutureExt, Shared};

use crate::executor::{panic_message, ExecutorError, ExecutorLifecycle, ResourceBlockingExecutor};

/// Type alias for the complex job function type used by SingleThreadExecutor
type JobFn = Box<dyn FnOnce(&mut dyn Any) + Send>;

/// Single-threaded executor that runs blocking jobs on a dedicated worker thread,
/// providing mutable access to executor-owned resources.
///
/// This is your strict FIFO lane for jobs that must serialize access to a
/// single resource (e.g. a device or legacy client that explodes under
/// concurrency).
pub struct SingleThreadExecutor<R = ()> {
    state: Arc<State>,
    resource_type_id: TypeId,
    _marker: PhantomData<R>,
}

impl Default for SingleThreadExecutor<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleThreadExecutor<()> {
    #[must_use]
    pub fn new() -> Self {
        Self::with_resources(())
    }
}

impl<R> SingleThreadExecutor<R>
where
    R: Send + Sync + 'static,
{
    #[must_use]
    pub fn with_resources(resources: R) -> Self {
        let (tx, rx) = unbounded::<Job>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("syzygy-single-executor".into())
            .spawn(move || worker_loop(rx, shutdown_tx, resources))
            .expect("failed to spawn single-thread executor");

        let completed_shutdown = async move {
            let _ = shutdown_rx.await;
        }
        .boxed()
        .shared();

        let state = State {
            tx,
            shutdown_requested: AtomicBool::new(false),
            completed_shutdown,
            thread: Mutex::new(Some(thread)),
        };

        Self {
            state: Arc::new(state),
            resource_type_id: TypeId::of::<R>(),
            _marker: PhantomData,
        }
    }
}

impl<R> SingleThreadExecutor<R>
where
    R: Send + Sync + 'static,
{
    /// Convenience helper for submitting typed jobs.
    pub fn spawn<F>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(&mut R) + Send + 'static,
    {
        let wrapped = Box::new(move |resource: &mut dyn Any| {
            let typed = resource
                .downcast_mut::<R>()
                .expect("single thread executor resource type mismatch");
            job(typed);
        });

        if self.state.shutdown_requested.load(Ordering::Acquire)
            || self.state.tx.send(Job::Work(wrapped)).is_err()
        {
            return Err(ExecutorError::WorkerGone);
        }

        Ok(())
    }
}

impl<E, R> ResourceBlockingExecutor<E> for SingleThreadExecutor<R>
where
    E: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    fn resource_type_id(&self) -> std::any::TypeId {
        self.resource_type_id
    }

    fn spawn_blocking_with_resource(
        &self,
        job: Box<dyn FnOnce(&mut dyn Any) + Send>,
    ) -> Result<(), ExecutorError> {
        if self.state.shutdown_requested.load(Ordering::Acquire)
            || self.state.tx.send(Job::Work(job)).is_err()
        {
            return Err(ExecutorError::WorkerGone);
        }

        Ok(())
    }
}

impl<R> ExecutorLifecycle for SingleThreadExecutor<R>
where
    R: Send + Sync + 'static,
{
    fn shutdown(&self) {
        if !self.state.shutdown_requested.swap(true, Ordering::AcqRel) {
            let _ = self.state.tx.send(Job::Shutdown);
        }
    }

    fn wait(&self) {
        self.shutdown();
        let fut = self.state.completed_shutdown.clone();
        block_on(async {
            let () = fut.await;
        });
    }
}

struct State {
    tx: Sender<Job>,
    shutdown_requested: AtomicBool,
    completed_shutdown: Shared<BoxFuture<'static, ()>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for State {
    fn drop(&mut self) {
        if !self.shutdown_requested.swap(true, Ordering::AcqRel) {
            let _ = self.tx.send(Job::Shutdown);
        }

        let _ = self.completed_shutdown.clone().now_or_never();

        if let Ok(mut guard) = self.thread.lock() {
            if let Some(join) = guard.take() {
                let _ = join.join();
            }
        }
    }
}

enum Job {
    Work(JobFn),
    Shutdown,
}

fn worker_loop<R>(rx: Receiver<Job>, shutdown_tx: oneshot::Sender<()>, mut resources: R)
where
    R: Send + Sync + 'static,
{
    while let Ok(job) = rx.recv() {
        match job {
            Job::Work(job) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job(&mut resources as &mut dyn Any);
                }));

                if let Err(panic) = result {
                    let _ = panic_message(panic);
                }
            }
            Job::Shutdown => break,
        }
    }

    let _ = shutdown_tx.send(());
}
