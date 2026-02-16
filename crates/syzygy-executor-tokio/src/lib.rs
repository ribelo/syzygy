use futures::executor::block_on;
use futures_util::{
    future::{BoxFuture, FutureExt},
    TryFutureExt,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use syzygy::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle};
use tokio::runtime::{self, Handle};
use tokio::sync::{oneshot::error::RecvError, Notify};

/// Executor backed by a dedicated tokio runtime on its own thread.
#[derive(Debug)]
pub struct TokioExecutor {
    driver: Driver,
}

#[derive(Debug)]
pub struct TokioExecutorBuilder {
    name: String,
    runtime_kind: RuntimeKind,
    worker_threads: Option<usize>,
    enable_time: bool,
    enable_io: bool,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeKind {
    MultiThread,
    CurrentThread,
}

#[derive(Debug)]
enum Driver {
    Owned(Arc<RwLock<OwnedState>>),
    Attached(Handle),
}

#[derive(Debug)]
struct OwnedState {
    handle: Option<Handle>,
    notify_shutdown: Arc<Notify>,
    completed_shutdown:
        futures_util::future::Shared<BoxFuture<'static, Result<(), Arc<RecvError>>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const NO_RUNTIME_MSG: &str =
    "No tokio runtime running. Use #[tokio::main] or create a runtime first.";

impl Drop for OwnedState {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(handle);
            self.notify_shutdown.notify_one();
        }

        if let Some(join) = self.thread.take() {
            let _ = join.join();
        }
    }
}

impl TokioExecutor {
    pub fn new_with(name: &str, mut builder: runtime::Builder) -> Self {
        let notify_shutdown = Arc::new(Notify::new());
        let notify_clone = Arc::clone(&notify_shutdown);

        let (tx_handle, rx_handle) = std::sync::mpsc::channel::<Handle>();
        let (tx_shutdown, rx_shutdown) = tokio::sync::oneshot::channel::<()>();

        let thread = std::thread::Builder::new()
            .name(format!("{name} executor"))
            .spawn(move || {
                let rt = builder.build().expect("failed to build tokio runtime");

                rt.block_on(async move {
                    let _ = tx_handle.send(Handle::current());
                    notify_clone.notified().await;
                });
                rt.shutdown_background();
                let _ = tx_shutdown.send(());
            })
            .expect("failed to spawn tokio executor thread");

        let handle = rx_handle.recv().expect("executor thread failed to start");

        let state = OwnedState {
            handle: Some(handle),
            notify_shutdown,
            completed_shutdown: futures_util::FutureExt::boxed(rx_shutdown.map_err(Arc::new))
                .shared(),
            thread: Some(thread),
        };

        Self {
            driver: Driver::Owned(Arc::new(RwLock::new(state))),
        }
    }

    #[must_use]
    pub fn multi_thread_io(name: &str, worker_threads: usize) -> Self {
        Self::builder()
            .name(name)
            .multi_thread()
            .worker_threads(worker_threads)
            .io()
            .build()
    }

    #[must_use]
    pub fn current_thread_io(name: &str) -> Self {
        Self::builder().name(name).current_thread().io().build()
    }

    #[must_use]
    pub fn multi_thread_cpu(name: &str, worker_threads: usize) -> Self {
        Self::builder()
            .name(name)
            .multi_thread()
            .worker_threads(worker_threads)
            .cpu()
            .build()
    }

    #[must_use]
    pub fn current_thread_cpu(name: &str) -> Self {
        Self::builder().name(name).current_thread().cpu().build()
    }

    #[must_use]
    pub fn from_handle(handle: Handle) -> Self {
        Self {
            driver: Driver::Attached(handle),
        }
    }

    pub fn try_from_current() -> Result<Self, &'static str> {
        Handle::try_current()
            .map(Self::from_handle)
            .map_err(|_| NO_RUNTIME_MSG)
    }

    #[must_use]
    pub fn builder() -> TokioExecutorBuilder {
        TokioExecutorBuilder::new()
    }
}

impl AsyncExecutor for TokioExecutor {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        match &self.driver {
            Driver::Owned(state) => {
                let handle = {
                    let guard = state.read().expect("executor state poisoned");
                    guard.handle.clone()
                };

                let Some(handle) = handle else {
                    return Err(ExecutorError::WorkerGone);
                };

                handle.spawn(job);
                Ok(())
            }
            Driver::Attached(handle) => {
                handle.spawn(job);
                Ok(())
            }
        }
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        tokio::time::sleep(duration).boxed()
    }
}

impl ExecutorLifecycle for TokioExecutor {
    fn shutdown(&self) {
        if let Driver::Owned(state) = &self.driver {
            let mut guard = state.write().expect("executor state poisoned");
            if guard.handle.take().is_some() {
                guard.notify_shutdown.notify_one();
            }
        }
    }

    fn wait(&self) {
        match &self.driver {
            Driver::Owned(state) => {
                self.shutdown();
                let fut = {
                    let guard = state.read().expect("executor state poisoned");
                    guard.completed_shutdown.clone()
                };
                block_on(async {
                    let _ = fut.await;
                });
            }
            Driver::Attached(_) => {
                // nothing to wait for on attached runtime
            }
        }
    }
}

impl TokioExecutorBuilder {
    fn new() -> Self {
        Self {
            name: "syzygy-tokio".to_string(),
            runtime_kind: RuntimeKind::MultiThread,
            worker_threads: None,
            enable_time: true,
            enable_io: true,
        }
    }

    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    #[must_use]
    pub fn multi_thread(mut self) -> Self {
        self.runtime_kind = RuntimeKind::MultiThread;
        self
    }

    #[must_use]
    pub fn current_thread(mut self) -> Self {
        self.runtime_kind = RuntimeKind::CurrentThread;
        self
    }

    #[must_use]
    pub fn worker_threads(mut self, threads: usize) -> Self {
        self.worker_threads = Some(threads.max(1));
        self
    }

    #[must_use]
    pub fn io(mut self) -> Self {
        self.enable_io = true;
        self.enable_time = true;
        self
    }

    #[must_use]
    pub fn cpu(mut self) -> Self {
        self.enable_io = false;
        self.enable_time = true;
        self
    }

    #[must_use]
    pub fn enable_time(mut self, enable: bool) -> Self {
        self.enable_time = enable;
        if !self.enable_time {
            self.enable_io = false;
        }
        self
    }

    #[must_use]
    pub fn enable_io(mut self, enable: bool) -> Self {
        self.enable_io = enable;
        if self.enable_io {
            self.enable_time = true;
        }
        self
    }

    pub fn build(self) -> TokioExecutor {
        let mut builder = match self.runtime_kind {
            RuntimeKind::MultiThread => {
                let mut builder = runtime::Builder::new_multi_thread();
                let threads = self.worker_threads.unwrap_or_else(default_worker_threads);
                builder.worker_threads(threads);
                builder
            }
            RuntimeKind::CurrentThread => runtime::Builder::new_current_thread(),
        };

        if self.enable_io {
            builder.enable_all();
        } else if self.enable_time {
            builder.enable_time();
        }

        TokioExecutor::new_with(&self.name, builder)
    }
}

fn default_worker_threads() -> usize {
    match std::thread::available_parallelism() {
        Ok(threads) => threads.get().max(1),
        Err(_) => 1,
    }
}
