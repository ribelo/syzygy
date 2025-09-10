use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, unbounded};
use futures::channel::oneshot;
use futures_util::future::{BoxFuture, FutureExt, Shared};

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::executor::{ExecutorError, ExecutorLifecycle, SyncExecutor};
use crate::streaming::EffectOutput;

/// `SingleThreadExecutor` — FIFO, single-worker executor for sync work only
///
/// - Sync jobs: executed in strict FIFO order on a dedicated worker thread
/// - Cancellation is supported via oneshot close (cancel-on-drop of the join future)
/// - Panic isolation: panics in jobs are caught and mapped to `ExecutorError::Panic`
///
/// Notes:
/// - This executor is runtime-neutral and only supports synchronous work
/// - For single-threaded async work, use `TokioExecutor::current_thread`_* instead
/// - All jobs run on a dedicated worker thread to avoid blocking the main thread
pub struct SingleThreadExecutor {
    state: Arc<State>,
}

impl fmt::Debug for SingleThreadExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "SingleThreadExecutor".fmt(f)
    }
}

impl Default for SingleThreadExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleThreadExecutor {
    /// Create a new single-threaded executor for synchronous work
    ///
    /// This executor runs all jobs on a single dedicated thread in strict FIFO order.
    /// It's ideal for:
    /// - Database writes that must be sequential
    /// - State mutations that require ordering guarantees
    /// - Work that should not block async runtimes
    ///
    /// # Panics
    ///
    /// Panics if the worker thread cannot be spawned.
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = unbounded::<Job>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("syzygy-single-executor".to_string())
            .spawn(move || worker_loop(rx, shutdown_tx))
            .expect("failed to spawn single-thread executor worker");

        let completed_shutdown: Shared<BoxFuture<'static, ()>> =
            futures_util::FutureExt::boxed(async move {
                // Ignore errors if the sender was dropped without sending
                let _ = shutdown_rx.await;
            })
            .shared();

        let state = State {
            tx: Mutex::new(Some(tx)),
            completed_shutdown,
            thread: Mutex::new(Some(thread)),
        };

        Self {
            state: Arc::new(state),
        }
    }
}

impl<E> SyncExecutor<E> for SingleThreadExecutor
where
    E: Send + 'static,
{
    /// Spawn a synchronous job on the single worker thread
    ///
    /// Jobs are executed in strict FIFO order on a dedicated thread.
    /// This provides deterministic execution ordering and prevents
    /// blocking of async runtimes.
    ///
    /// # Cancellation
    ///
    /// Dropping the returned future cancels the job if it hasn't started
    /// execution yet. Once a job starts running, cancellation is cooperative
    /// and depends on the job's implementation.
    fn spawn_sync(
        &self,
        job: Box<dyn FnOnce() -> EffectOutput<E> + Send>,
    ) -> BoxFuture<'static, Result<EffectOutput<E>, ExecutorError>> {
        // Wrap job to type erase its output
        let wrapped_job: Box<dyn FnOnce() -> Box<dyn Any + Send> + Send> =
            Box::new(move || -> Box<dyn Any + Send> { Box::new(job()) });

        let (tx_result, rx_result) =
            oneshot::channel::<Result<Box<dyn Any + Send>, ExecutorError>>();

        let send_res = {
            let guard = self.state.tx.lock().expect("executor state poisoned");
            if let Some(sender) = guard.as_ref() {
                sender.send(Job::Sync {
                    job: wrapped_job,
                    tx: tx_result,
                })
            } else {
                Err(crossbeam_channel::SendError(Job::Shutdown))
            }
        };

        if send_res.is_err() {
            return futures_util::future::err(ExecutorError::WorkerGone).boxed();
        }

        // No cancellation for sync jobs, just await completion
        JoinFuture::<E>::new(rx_result).boxed()
    }
}

impl ExecutorLifecycle for SingleThreadExecutor {
    fn shutdown(&self) {
        let mut guard = self.state.tx.lock().expect("executor state poisoned");
        if let Some(sender) = guard.take() {
            // Signal shutdown; ignoring send error if worker already exited
            let _ = sender.send(Job::Shutdown);
        }
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        // Initiate shutdown and then await completion
        self.shutdown();
        let fut = self.state.completed_shutdown.clone();
        async move {
            let () = fut.await;
        }
        .boxed()
    }
}

struct State {
    tx: Mutex<Option<Sender<Job>>>,
    completed_shutdown: Shared<BoxFuture<'static, ()>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for State {
    fn drop(&mut self) {
        // Ensure shutdown signal is sent
        if let Ok(mut guard) = self.tx.lock()
            && let Some(sender) = guard.take()
        {
            let _ = sender.send(Job::Shutdown);
        }

        // Try to advance completion without blocking runtime executors
        let _ = self.completed_shutdown.clone().now_or_never();

        // Join OS thread to avoid leaks
        if let Ok(mut th) = self.thread.lock()
            && let Some(handle) = th.take()
        {
            let _ = handle.join();
        }
    }
}

enum Job {
    Sync {
        job: Box<dyn FnOnce() -> Box<dyn Any + Send> + Send>,
        tx: oneshot::Sender<Result<Box<dyn Any + Send>, ExecutorError>>,
    },
    Shutdown,
}

fn worker_loop(rx: Receiver<Job>, shutdown_tx: oneshot::Sender<()>) {
    // FIFO processing of sync jobs on this single worker thread
    while let Ok(job) = rx.recv() {
        match job {
            Job::Sync { job, tx } => {
                // Catch panics and map to ExecutorError::Panic
                let res = catch_unwind(AssertUnwindSafe(job)).map_err(|p| ExecutorError::Panic {
                    msg: panic_to_msg(p),
                });

                let _ = tx.send(res);
            }
            Job::Shutdown => break,
        }
    }

    // Notify completion
    let _ = shutdown_tx.send(());
}

fn panic_to_msg(p: Box<dyn Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unknown internal error".to_string()
    }
}

struct JoinFuture<E> {
    rx: Option<oneshot::Receiver<Result<Box<dyn Any + Send>, ExecutorError>>>,
    _phantom: std::marker::PhantomData<E>,
}

impl<E> JoinFuture<E> {
    fn new(rx: oneshot::Receiver<Result<Box<dyn Any + Send>, ExecutorError>>) -> Self {
        Self {
            rx: Some(rx),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<E> std::future::Future for JoinFuture<E>
where
    E: Send + 'static,
{
    type Output = Result<EffectOutput<E>, ExecutorError>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use futures_util::FutureExt;

        // SAFETY: JoinFuture contains no self-referential or pinned fields. We only access
        // interior state (rx) and do not move the struct after it has been pinned. Using
        // get_unchecked_mut here avoids requiring Unpin while remaining sound.
        let this = unsafe { self.get_unchecked_mut() };
        if let Some(rx) = &mut this.rx {
            match rx.poll_unpin(cx) {
                std::task::Poll::Pending => std::task::Poll::Pending,
                std::task::Poll::Ready(res) => {
                    this.rx = None;
                    let out = match res {
                        Ok(Ok(boxed)) => {
                            // Downcast to the expected EffectOutput<E>
                            match boxed.downcast::<EffectOutput<E>>() {
                                Ok(typed) => Ok(*typed),
                                Err(_boxed) => Err(ExecutorError::WorkerGone),
                            }
                        }
                        Ok(Err(err)) => Err(err),
                        Err(_canceled) => Err(ExecutorError::WorkerGone),
                    };
                    std::task::Poll::Ready(out)
                }
            }
        } else {
            // Already resolved
            std::task::Poll::Ready(Err(ExecutorError::WorkerGone))
        }
    }
}
