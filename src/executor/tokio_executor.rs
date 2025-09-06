//! TokioExecutor — dedicated tokio runtime with shutdown/join
//!
//! Configurable executor inspired by InfluxDB's DedicatedExecutor:
//! - IO vs CPU-only modes (via runtime builder enablement)
//! - Multi-threaded vs current-thread
//! - Cancel-on-drop join futures and clean shutdown
//!
//! Note: The executor API is intentionally minimal; task tracking is not
//! exposed. Dropping the executor (or calling `shutdown`) stops the runtime
//! and cancels outstanding tasks.

use crate::executor::{ExecutorError, Executor};

use futures_util::future::{BoxFuture, FutureExt};
use std::future::Future;
use std::sync::{Arc, RwLock};

#[cfg(feature = "tokio")]
use tokio::{
    runtime::{self, Handle},
    sync::{oneshot::error::RecvError, Notify},
    task::JoinSet,
};

/// Executor backed by a dedicated tokio runtime on its own thread.
#[derive(Clone)]
pub struct TokioExecutor {
    state: Arc<RwLock<State>>, // shared across clones
}

/// Inner state managed by the worker thread lifecycle.
#[cfg(feature = "tokio")]
struct State {
    /// None once shutdown has started
    handle: Option<Handle>,
    /// Notify the worker to begin shutdown
    start_shutdown: Arc<Notify>,
    /// Completed shutdown signal
    completed_shutdown: futures_util::future::Shared<
        BoxFuture<'static, Result<(), Arc<RecvError>>>,
    >,
    /// Join handle for worker thread
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "tokio")]
impl Drop for State {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.handle = None;
            self.start_shutdown.notify_one();
        }

        // Don't poll the shared future if panicking
        if !std::thread::panicking() {
            let _ = self.completed_shutdown.clone().now_or_never();
        }

        // Join the worker thread, ignore result
        if let Some(join) = self.thread.take() {
            let _ = join.join();
        }
    }
}

impl std::fmt::Debug for TokioExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TokioExecutor")
    }
}

impl TokioExecutor {
    /// Build a new TokioExecutor from a pre-configured runtime builder.
    ///
    /// To create an IO-capable multi-threaded executor:
    /// - `runtime::Builder::new_multi_thread().enable_all()`
    ///
    /// To create a CPU-only (no IO) executor:
    /// - call only `enable_time()` on the builder.
    #[cfg(feature = "tokio")]
    pub fn new(name: &str, mut runtime_builder: runtime::Builder) -> Self {
        let name = name.to_owned();

        let notify_shutdown = Arc::new(Notify::new());
        let notify_shutdown_captured = Arc::clone(&notify_shutdown);

        let (tx_shutdown, rx_shutdown) = tokio::sync::oneshot::channel::<()>();
        let (tx_handle, rx_handle) = std::sync::mpsc::channel::<Handle>();

        // Optionally capture current runtime for IO registration in threads
        let io_handle = runtime::Handle::try_current().ok();

        let thread = std::thread::Builder::new()
            .name(format!("{name} executor"))
            .spawn(move || {
                // Build the runtime
                let rt = runtime_builder
                    .on_thread_start(move || {
                        // Placeholder hook for IO runtime registration if desired
                        let _ = &io_handle;
                    })
                    .build()
                    .expect("creating tokio runtime");

                // Drive until shutdown
                rt.block_on(async move {
                    // Enable the notified future before sending handle back
                    let shutdown = notify_shutdown_captured.notified();
                    let mut shutdown = std::pin::pin!(shutdown);
                    shutdown.as_mut().enable();

                    if tx_handle.send(runtime::Handle::current()).is_err() {
                        return;
                    }
                    shutdown.await;
                });

                // Best-effort graceful shutdown
                rt.shutdown_timeout(std::time::Duration::from_secs(5 * 60));
                let _ = tx_shutdown.send(());
            })
            .expect("executor setup");

        let handle = rx_handle.recv().expect("worker started");

        let state = State {
            handle: Some(handle),
            start_shutdown: notify_shutdown,
            completed_shutdown: futures_util::FutureExt::boxed(
                rx_shutdown.map_err(Arc::new),
            )
            .shared(),
            thread: Some(thread),
        };

        Self { state: Arc::new(RwLock::new(state)) }
    }

    /// Convenience constructor: multi-threaded executor with IO enabled.
    #[cfg(feature = "tokio")]
    pub fn multi_thread_io(name: &str, worker_threads: usize) -> Self {
        let mut b = runtime::Builder::new_multi_thread();
        b.worker_threads(worker_threads).enable_all();
        Self::new(name, b)
    }

    /// Convenience constructor: current-thread executor with IO enabled.
    #[cfg(feature = "tokio")]
    pub fn current_thread_io(name: &str) -> Self {
        let mut b = runtime::Builder::new_current_thread();
        b.enable_all();
        Self::new(name, b)
    }

    /// Convenience: multi-threaded CPU-heavy executor (no IO integration).
    #[cfg(feature = "tokio")]
    pub fn multi_thread_cpu(name: &str, worker_threads: usize) -> Self {
        let mut b = runtime::Builder::new_multi_thread();
        b.worker_threads(worker_threads).enable_time();
        Self::new(name, b)
    }

    /// Convenience: current-thread CPU-heavy executor (no IO integration).
    #[cfg(feature = "tokio")]
    pub fn current_thread_cpu(name: &str) -> Self {
        let mut b = runtime::Builder::new_current_thread();
        b.enable_time();
        Self::new(name, b)
    }
}

#[cfg(feature = "tokio")]
impl Executor for TokioExecutor {
    fn spawn<T>(&self, task: T) -> BoxFuture<'static, Result<T::Output, ExecutorError>>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        // Capture handle under read lock
        let handle = {
            let guard = self.state.read().expect("executor state poisoned");
            guard.handle.clone()
        };

        let Some(handle) = handle else {
            return futures_util::future::err(ExecutorError::WorkerGone).boxed();
        };

        // Cancel-on-drop via JoinSet
        let mut join_set = JoinSet::new();
        join_set.spawn_on(task, &handle);

        async move {
            match join_set.join_next().await.expect("just spawned task") {
                Ok(output) => Ok(output),
                Err(join_err) => match join_err.try_into_panic() {
                    Ok(p) => {
                        let msg = if let Some(s) = p.downcast_ref::<String>() {
                            s.clone()
                        } else if let Some(s) = p.downcast_ref::<&str>() {
                            s.to_string()
                        } else {
                            "unknown internal error".to_string()
                        };
                        Err(ExecutorError::Panic { msg })
                    }
                    Err(_) => Err(ExecutorError::WorkerGone),
                },
            }
        }
        .boxed()
    }

    fn shutdown(&self) {
        let mut guard = self.state.write().expect("executor state poisoned");
        if guard.handle.take().is_some() {
            guard.start_shutdown.notify_one();
        }
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        let fut = {
            let guard = self.state.read().expect("executor state poisoned");
            guard.completed_shutdown.clone()
        };
        async move {
            let _ = fut.await;
        }
        .boxed()
    }
}

#[cfg(not(feature = "tokio"))]
impl Executor for TokioExecutor {
    fn spawn<T>(&self, _task: T) -> BoxFuture<'static, Result<T::Output, ExecutorError>>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        futures_util::future::err(ExecutorError::WorkerGone).boxed()
    }

    fn shutdown(&self) {}
    fn join(&self) -> BoxFuture<'static, ()> { futures_util::future::ready(()).boxed() }
}
