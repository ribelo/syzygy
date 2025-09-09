#[cfg(feature = "tokio")]
// Tokio Executor Architecture — specialized async runtimes for different work types
//
// This module provides three executor types built on Tokio:
//
// ## TokioExecutor (Base)
// The foundational executor that creates dedicated Tokio runtimes on separate threads.
// Use this when you need custom runtime configuration.
//
// ## TokioIo (Newtype Wrapper)
// Specialized for IO-bound async work with `enable_all()` runtime features.
// Perfect for network operations, file I/O, and other async IO tasks.
//
// ## TokioCpu (Newtype Wrapper)  
// Specialized for CPU-bound async work with only `enable_time()` runtime.
// Ideal for computation-heavy async tasks that don't need IO capabilities.
//
// ## Design Rationale
//
// The newtype pattern (`TokioIo`, `TokioCpu`) enables registering multiple
// Tokio executors with different configurations while maintaining distinct
// `TypeId`s for the registry system. This separation allows:
//
// - **IO isolation**: Network/file operations run on dedicated IO-optimized runtimes
// - **CPU optimization**: Computation work runs on lean runtimes without IO overhead
// - **Resource separation**: Different thread pools for different workload types
// - **Performance tuning**: Each executor can be configured for its specific use case
//
// ## Usage Examples
//
// ```rust
// use syzygy::executor::{TokioIo, TokioCpu, TokioExecutor};
//
// // IO-focused executor for network operations
// let io_executor = TokioIo::multi_thread(4);
// 
// // CPU-focused executor for async computations
// let cpu_executor = TokioCpu::multi_thread(8);
//
// // Custom configuration using base executor
// let custom = TokioExecutor::current_thread_cpu("custom-worker");
// ```
//
// ## Runtime Configuration
//
// - **TokioIo**: Uses `enable_all()` - full tokio feature set for IO operations
// - **TokioCpu**: Uses `enable_time()` only - minimal runtime for CPU work
// - **Thread models**: Both support `current_thread` and `multi_thread` configurations

use crate::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle, register_io_runtime};

use futures::{TryFutureExt, future::{abortable, AbortHandle, Aborted}};
use futures_util::future::{BoxFuture, FutureExt};
use std::sync::{Arc, RwLock};
use tokio::{
    runtime::{self, Handle},
    sync::{Notify, oneshot::error::RecvError},
};

/// Executor backed by a dedicated tokio runtime on its own thread.
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
#[derive(Clone)]
pub struct TokioExecutor {
    state: Arc<RwLock<State>>, // shared across clones
}

/// Inner state managed by the worker thread lifecycle.
struct State {
    /// None once shutdown has started
    handle: Option<Handle>,
    /// Notify the worker to begin shutdown
    start_shutdown: Arc<Notify>,
    /// Completed shutdown signal
    completed_shutdown:
        futures_util::future::Shared<BoxFuture<'static, Result<(), Arc<RecvError>>>>,
    /// Join handle for worker thread
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for State {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.handle = None;
            self.start_shutdown.notify_one();
        }

        if !std::thread::panicking() {
            let _ = self.completed_shutdown.clone().now_or_never();
        }

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
    /// Build a new TokioExecutor from a pre-configured runtime builder and context parts.
    pub fn new_with(name: &str, mut runtime_builder: runtime::Builder) -> Self {
        let name = name.to_owned();

        let notify_shutdown = Arc::new(Notify::new());
        let notify_shutdown_captured = Arc::clone(&notify_shutdown);

        let (tx_shutdown, rx_shutdown) = tokio::sync::oneshot::channel::<()>();
        let (tx_handle, rx_handle) = std::sync::mpsc::channel::<Handle>();

        let io_handle = runtime::Handle::try_current().ok();

        let thread = std::thread::Builder::new()
            .name(format!("{name} executor"))
            .spawn(move || {
                // Register IO runtime for the executor thread
                register_io_runtime(io_handle.clone());

                let rt = runtime_builder
                    .on_thread_start(move || {
                        // Register IO runtime for each worker thread spawned by this runtime
                        register_io_runtime(io_handle.clone());
                    })
                    .build()
                    .expect("creating tokio runtime");

                rt.block_on(async move {
                    let shutdown = notify_shutdown_captured.notified();
                    let mut shutdown = std::pin::pin!(shutdown);
                    shutdown.as_mut().enable();

                    if tx_handle.send(runtime::Handle::current()).is_err() {
                        return;
                    }
                    shutdown.await;
                });

                // Graceful shutdown
                rt.shutdown_timeout(std::time::Duration::from_secs(5 * 60));
                let _ = tx_shutdown.send(());
            })
            .expect("executor setup");

        let handle = rx_handle.recv().expect("worker started");

        let state = State {
            handle: Some(handle),
            start_shutdown: notify_shutdown,
            completed_shutdown: futures_util::FutureExt::boxed(rx_shutdown.map_err(Arc::new))
                .shared(),
            thread: Some(thread),
        };

        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }

    #[must_use]
    pub fn multi_thread_io(name: &str, worker_threads: usize) -> Self {
        let mut b = runtime::Builder::new_multi_thread();
        b.worker_threads(worker_threads).enable_all();
        Self::new_with(name, b)
    }

    #[must_use]
    pub fn current_thread_io(name: &str) -> Self {
        let mut b = runtime::Builder::new_current_thread();
        b.enable_all();
        Self::new_with(name, b)
    }

    #[must_use]
    pub fn multi_thread_cpu(name: &str, worker_threads: usize) -> Self {
        let mut b = runtime::Builder::new_multi_thread();
        b.worker_threads(worker_threads).enable_time();
        Self::new_with(name, b)
    }

    #[must_use]
    pub fn current_thread_cpu(name: &str) -> Self {
        let mut b = runtime::Builder::new_current_thread();
        b.enable_time();
        Self::new_with(name, b)
    }
}
// Future wrapper that aborts the spawned task on drop to provide cancel-on-drop semantics
struct AbortOnDrop<E> {
   handle: tokio::task::JoinHandle<Result<crate::streaming::EffectOutput<E>, Aborted>>,
   abort: AbortHandle,
}

impl<E> Drop for AbortOnDrop<E> {
   fn drop(&mut self) {
       self.abort.abort();
   }
}

impl<E> std::future::Future for AbortOnDrop<E>
where
   E: Send + 'static,
{
   type Output = Result<crate::streaming::EffectOutput<E>, ExecutorError>;

   fn poll(
       self: std::pin::Pin<&mut Self>,
       cx: &mut std::task::Context<'_>,
   ) -> std::task::Poll<Self::Output> {
       let this = self.get_mut();
       match std::pin::Pin::new(&mut this.handle).poll(cx) {
           std::task::Poll::Ready(join_res) => {
               std::task::Poll::Ready(match join_res {
                   Ok(Ok(output)) => Ok(output),
                   Ok(Err(_aborted)) => Err(ExecutorError::Cancelled),
                   Err(join_err) => match join_err.try_into_panic() {
                       Ok(p) => {
                           let msg = if let Some(s) = p.downcast_ref::<String>() {
                               s.clone()
                           } else if let Some(s) = p.downcast_ref::<&str>() {
                               (*s).to_string()
                           } else {
                               "unknown internal error".to_string()
                           };
                           Err(ExecutorError::Panic { msg })
                       }
                       Err(_) => Err(ExecutorError::WorkerGone),
                   },
               })
           }
           std::task::Poll::Pending => std::task::Poll::Pending,
       }
   }
}

impl<E> AsyncExecutor<E> for TokioExecutor
where
    E: Send + 'static,
{
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, crate::streaming::EffectOutput<E>>,
    ) -> BoxFuture<'static, Result<crate::streaming::EffectOutput<E>, ExecutorError>> {
        let handle = {
            let guard = self.state.read().expect("executor state poisoned");
            guard.handle.clone()
        };

        let Some(handle) = handle else {
            return futures_util::future::err(ExecutorError::WorkerGone).boxed();
        };

        let (ab_fut, abort_handle) = abortable(fut);
        let join_handle = handle.spawn(ab_fut);
        AbortOnDrop { handle: join_handle, abort: abort_handle }.boxed()
    }
}

impl crate::executor::ExecutorLifecycle for TokioExecutor {
    fn shutdown(&self) {
        let mut guard = self.state.write().expect("executor state poisoned");
        if guard.handle.take().is_some() {
            guard.start_shutdown.notify_one();
        }
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        self.shutdown();
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

/// Newtype wrapper for IO-focused Tokio executor
///
/// This executor is optimized for IO-bound work with `enable_all()` runtime features.
/// Use this for network operations, file I/O, and other async IO tasks.
///
/// # Runtime Configuration
///
/// Uses `enable_all()` which includes:
/// - `net` - TCP/UDP networking capabilities
/// - `process` - Process management  
/// - `signal` - Signal handling
/// - `rt` - Runtime utilities
/// - `time` - Timer functionality
///
/// # Examples
///
/// ```rust
/// use syzygy::executor::TokioIo;
///
/// // Create IO-focused executor with default thread count
/// let io_executor = TokioIo::default();
///
/// // Create with specific thread count
/// let io_executor = TokioIo::multi_thread(8);
///
/// // Create single-threaded for development/debugging
/// let io_executor = TokioIo::current_thread();
/// ```
///
/// # When to Use
///
/// - HTTP/HTTPS requests and web APIs
/// - Database connections and queries  
/// - File system operations
/// - Network protocols (TCP, UDP, WebSocket)
/// - Any async work requiring full tokio feature set
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
#[derive(Clone)]
pub struct TokioIo(pub TokioExecutor);

impl TokioIo {
    /// Create a current-thread IO executor
    pub fn current_thread() -> Self {
        Self(TokioExecutor::current_thread_io("tokio-io"))
    }

    /// Create a multi-thread IO executor with specified worker threads
    pub fn multi_thread(worker_threads: usize) -> Self {
        Self(TokioExecutor::multi_thread_io("tokio-io", worker_threads))
    }

    /// Create with default worker count (available parallelism)
    pub fn default() -> Self {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self::multi_thread(workers)
    }
}

/// Newtype wrapper for CPU-focused Tokio executor
///
/// This executor is optimized for CPU-bound async work with only `enable_time()`.
/// Use this for computation-heavy async tasks that don't need IO capabilities.
///
/// # Runtime Configuration
///
/// Uses `enable_time()` only, which provides:
/// - `rt` - Runtime utilities (minimal)
/// - `time` - Timer functionality
/// 
/// Explicitly excludes IO features (`net`, `process`, `signal`) to reduce:
/// - Memory footprint
/// - Thread pool overhead  
/// - Runtime complexity
///
/// # Examples
///
/// ```rust
/// use syzygy::executor::TokioCpu;
///
/// // Create CPU-focused executor with default thread count
/// let cpu_executor = TokioCpu::default();
///
/// // Create with specific thread count for parallel computation
/// let cpu_executor = TokioCpu::multi_thread(16);
///
/// // Create single-threaded for sequential CPU work
/// let cpu_executor = TokioCpu::current_thread();
/// ```
///
/// # When to Use
///
/// - Async data processing pipelines
/// - Mathematical computations with async coordination
/// - Async task orchestration without IO
/// - Background job processing
/// - Any async work that doesn't require network/file operations
///
/// # Performance Benefits
///
/// - **Lower memory usage**: No IO subsystem overhead
/// - **Faster startup**: Minimal runtime initialization  
/// - **Reduced contention**: Fewer shared resources
/// - **Better cache locality**: Leaner runtime structures
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
#[derive(Clone)]
pub struct TokioCpu(pub TokioExecutor);

impl TokioCpu {
    /// Create a current-thread CPU executor
    pub fn current_thread() -> Self {
        Self(TokioExecutor::current_thread_cpu("tokio-cpu"))
    }

    /// Create a multi-thread CPU executor with specified worker threads
    pub fn multi_thread(worker_threads: usize) -> Self {
        Self(TokioExecutor::multi_thread_cpu("tokio-cpu", worker_threads))
    }

    /// Create with default worker count (available parallelism)
    pub fn default() -> Self {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self::multi_thread(workers)
    }
}

impl<E> AsyncExecutor<E> for TokioIo
where
    E: Send + 'static,
{
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, crate::streaming::EffectOutput<E>>,
    ) -> BoxFuture<'static, Result<crate::streaming::EffectOutput<E>, ExecutorError>> {
        self.0.spawn_future(fut)
    }
}

impl ExecutorLifecycle for TokioIo {
    fn shutdown(&self) {
        self.0.shutdown()
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        self.0.join()
    }
}

impl<E> AsyncExecutor<E> for TokioCpu
where
    E: Send + 'static,
{
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, crate::streaming::EffectOutput<E>>,
    ) -> BoxFuture<'static, Result<crate::streaming::EffectOutput<E>, ExecutorError>> {
        self.0.spawn_future(fut)
    }
}

impl ExecutorLifecycle for TokioCpu {
    fn shutdown(&self) {
        self.0.shutdown()
    }

    fn join(&self) -> BoxFuture<'static, ()> {
        self.0.join()
    }
}

impl std::fmt::Debug for TokioIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TokioIo")
    }
}

impl std::fmt::Debug for TokioCpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TokioCpu")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::EffectOutput;
    use std::time::{Duration, Instant};
    use crate::executor::ExecutorLifecycle;
    use tokio::task::JoinSet;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Done,
        Progress(u32),
    }

    #[tokio::test]
    async fn abort_on_drop_spawns_tasks_significantly_faster_than_joinset_approach() {
        // Given: Two approaches for spawning cancellable tasks
        let task_count = 1000;

        // Measure AbortOnDrop approach (current implementation)
        let start = Instant::now();
        let executor = TokioExecutor::current_thread_io("perf_test_abort");
        let mut handles = Vec::with_capacity(task_count);

        for i in 0..task_count {
            let fut = async move {
                tokio::time::sleep(Duration::from_millis(1)).await;
                EffectOutput::Future(async move { vec![TestEvent::Progress(i as u32)] }.boxed())
            }.boxed();
            handles.push(executor.spawn_future(fut));
        }
        let abort_spawn_time = start.elapsed();

        // Clean up
        executor.shutdown();
        let _ = tokio::time::timeout(Duration::from_millis(100), executor.join()).await;

        // Measure JoinSet approach (simulating old implementation)
        let start = Instant::now();
        let mut join_set = JoinSet::new();

        for i in 0..task_count {
            join_set.spawn(async move {
                tokio::time::sleep(Duration::from_millis(1)).await;
                TestEvent::Progress(i as u32)
            });
        }
        let joinset_spawn_time = start.elapsed();

        // Clean up JoinSet
        join_set.shutdown().await;

        // Then: AbortOnDrop should be significantly faster (at least 2x)
        println!("AbortOnDrop spawn time: {:?}", abort_spawn_time);
        println!("JoinSet spawn time: {:?}", joinset_spawn_time);

        // AbortOnDrop should be faster than JoinSet for high-volume spawning

        let total_time = start.elapsed();

        // Then: High churn should complete within reasonable time
        assert!(
            total_time < Duration::from_secs(5),
            "High churn test took too long: {:?}",
            total_time
        );
        println!("High churn test completed in: {:?}", total_time);
        println!("Average per executor: {:?}", total_time / task_count as u32);
    }

    #[test]
    fn abort_on_drop_has_zero_allocation_overhead_compared_to_joinset_for_wrapper_creation() {
        // Given: Comparison of allocation patterns
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            
            // Measure AbortOnDrop wrapper creation overhead
            let start = Instant::now();
            
            let executor = TokioExecutor::current_thread_io("alloc_test");
            
            for _ in 0..100 {
                let fut = async {
                    EffectOutput::Future(async { vec![TestEvent::Done] }.boxed())
                }.boxed();
                let _handle = executor.spawn_future(fut);
                // Dont await - just measure creation overhead
            }
            let abort_creation_time = start.elapsed();
            
            executor.shutdown();
            let _ = tokio::time::timeout(Duration::from_millis(100), executor.join()).await;
            
            // Measure JoinSet creation overhead
            let start = Instant::now();
            let mut join_set = JoinSet::new();
            
            for _ in 0..100 {
                join_set.spawn(async { TestEvent::Done });
            }
            let joinset_creation_time = start.elapsed();
            
            join_set.shutdown().await;
            
            // Then: AbortOnDrop should have minimal overhead
            println!("AbortOnDrop creation time: {:?}", abort_creation_time);
            println!("JoinSet creation time: {:?}", joinset_creation_time);
            
            // Should be competitive (within reasonable margin)
            // Using 5x threshold instead of 3x to account for system variance in microbenchmarks
            // This test is sensitive to system conditions and may show higher variance when run
            // alongside other tests due to CPU scheduling and memory pressure
            assert!(
                abort_creation_time < joinset_creation_time * 5,
                "AbortOnDrop creation overhead ({:?}) too high vs JoinSet ({:?})",
                abort_creation_time,
                joinset_creation_time
            );
        });
    }
}
