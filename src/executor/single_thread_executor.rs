use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, unbounded};
use futures::channel::oneshot;
use futures_util::future::{BoxFuture, FutureExt, Shared};

use crate::executor::{ExecutorError, ExecutorLifecycle, SyncBorrowedExecutor, panic_message};

/// Single-threaded executor that runs blocking jobs on a dedicated worker thread,
/// providing mutable access to executor-owned resources.
pub struct SingleThreadExecutor<S = ()> {
    state: Arc<State<S>>,
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
        let (tx, rx) = unbounded::<Job<R>>();
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
        }
    }
}

impl<E, R> SyncBorrowedExecutor<E> for SingleThreadExecutor<R>
where
    E: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    type Resources = R;

    fn spawn_sync<F>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(&mut Self::Resources) + Send + 'static,
    {
        if self.state.shutdown_requested.load(Ordering::Acquire)
            || self.state.tx.send(Job::Sync(Box::new(job))).is_err()
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

    fn join(&self) -> BoxFuture<'static, ()> {
        self.shutdown();
        let fut = self.state.completed_shutdown.clone();
        async move {
            let () = fut.await;
        }
        .boxed()
    }
}

struct State<R> {
    tx: Sender<Job<R>>,
    shutdown_requested: AtomicBool,
    completed_shutdown: Shared<BoxFuture<'static, ()>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl<R> Drop for State<R> {
    fn drop(&mut self) {
        if !self.shutdown_requested.swap(true, Ordering::AcqRel) {
            let _ = self.tx.send(Job::Shutdown);
        }

        let _ = self.completed_shutdown.clone().now_or_never();

        if let Ok(mut guard) = self.thread.lock()
            && let Some(join) = guard.take()
        {
            let _ = join.join();
        }
    }
}

enum Job<R> {
    Sync(Box<dyn FnOnce(&mut R) + Send>),
    Shutdown,
}

fn worker_loop<R>(rx: Receiver<Job<R>>, shutdown_tx: oneshot::Sender<()>, mut resources: R)
where
    R: Send + Sync + 'static,
{
    while let Ok(job) = rx.recv() {
        match job {
            Job::Sync(job) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job(&mut resources);
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
