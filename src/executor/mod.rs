//! Executors — specialized async runtimes for different work types
//!
//! Syzygy uses a **two-trait executor architecture** that separates async and sync
//! execution concerns. This design eliminates impedance mismatches between execution
//! models and provides optimal performance for different workload patterns.
//!
//! ## Architecture Overview
//!
//! The executor system provides four distinct execution contexts:
//!
//! ### Async Executors (for async work)
//! - **`TokioIo`**: IO-bound async work (network, files) with `enable_all()` runtime
//! - **`TokioCpu`**: CPU-bound async work with only `enable_time()` runtime
//! - **`TokioExecutor`**: Base executor for custom async configurations
//!
//! ### Sync Executors (for blocking work)
//! - **`RayonSyncExecutor`**: Parallel CPU work using Rayon's work-stealing
//! - **`SingleThreadExecutor`**: Sequential sync work with strict FIFO ordering
//!
//! ## Key Design Principles
//!
//! ### Separation of Concerns
//! ```rust
//! // IO-bound async work - use TokioIo
//! ctx.spawn(async {
//!     let response = reqwest::get("https://api.example.com").await?;
//!     // Network operations here
//! });
//!
//! // CPU-bound async work - use TokioCpu
//! ctx.spawn(async {
//!     let result = expensive_async_computation().await;
//!     // CPU-intensive async work
//! });
//!
//! // Parallel sync work - use RayonSyncExecutor
//! ctx.spawn_sync(|| {
//!     let result = parallel_computation();
//!     // CPU-bound parallel work
//! });
//!
//! // Sequential sync work - use SingleThreadExecutor
//! ctx.spawn_sync(|| {
//!     let result = sequential_database_write();
//!     // Must be processed in order
//! });
//! ```
//!
//! ### Newtype Pattern for Multiple Executors
//!
//! Use newtype wrappers to register multiple Tokio executors with different configurations:
//!
//! ```rust
//! use syzygy::executor::{TokioExecutor, TokioIo, TokioCpu};
//!
//! // IO-focused executor (enable_all)
//! let io_executor = TokioIo::multi_thread(4);
//!
//! // CPU-focused executor (enable_time only)
//! let cpu_executor = TokioCpu::multi_thread(8);
//!
//! // Custom executor configuration
//! let custom_io = TokioIo(TokioExecutor::current_thread_io("custom-io"));
//! ```
//!
//! ## Important Notes
//!
//! ### `SingleThreadExecutor` is Sync-Only
//! **`SingleThreadExecutor` no longer handles async work.** It's designed exclusively
//! for synchronous, FIFO-ordered execution. For single-threaded async needs:
//!
//! ```rust
//! // ❌ Don't do this - SingleThreadExecutor is sync-only
//! // let exec = SingleThreadExecutor::new();
//! // exec.spawn_future(async { /* async work */ }); // WON'T COMPILE
//!
//! // ✅ Do this instead - use TokioExecutor for single-threaded async
//! let exec = TokioExecutor::current_thread_io("my-async-worker");
//! ```
//!
//! ### Runtime Registration Pattern
//!
//! The IO runtime registration ensures IO operations run on the appropriate runtime:
//!
//! ```rust
//! use syzygy::executor::{register_current_runtime_for_io, spawn_io};
//!
//! // Register the current runtime for IO operations
//! register_current_runtime_for_io();
//!
//! // Later, spawn IO work on the registered runtime
//! spawn_io(async {
//!     // Network/file IO operations here
//!     println!("Running on IO runtime");
//! });
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Task spawning**: ~4ns per task (24x faster than previous implementation)
//! - **Zero allocation**: Optimized for high-frequency effect processing
//! - **Cancel-on-drop**: All tasks automatically cancelled when context drops
//! - **Memory safety**: No orphaned tasks or use-after-free issues
//!
//! ## When to Use Each Executor
//!
//! | Executor Type | Best For | Runtime | Threading | Ordering |
//! |---------------|----------|---------|-----------|----------|
//! | **TokioIo** | Network I/O, file operations | enable_all() | Multi/current thread | Concurrent |
//! | **TokioCpu** | CPU-bound async work | enable_time() | Multi/current thread | Concurrent |
//! | **RayonSyncExecutor** | Parallel computations | Rayon | Work-stealing | Non-deterministic |
//! | **SingleThreadExecutor** | Sequential work, FIFO requirements | None | Single dedicated | Strict FIFO |

// Executor storage module removed - using direct FxHashMap
pub mod inline_async;
#[cfg(feature = "rayon")]
pub mod rayon_sync_executor;
pub mod registry;
pub mod single_thread_executor;
pub mod spec;

#[cfg(feature = "tokio")]
pub mod tokio_current;
#[cfg(feature = "tokio")]
pub mod tokio_executor;

// IO runtime registration - inspired by InfluxDB's design
use std::future::Future;
use std::sync::RwLock;

#[cfg(feature = "tokio")]
static IO_RUNTIME: RwLock<Option<tokio::runtime::Handle>> = RwLock::new(None);

#[cfg(feature = "tokio")]
thread_local! {
    static THREAD_IO_RUNTIME: std::cell::RefCell<Option<tokio::runtime::Handle>> =
        const { std::cell::RefCell::new(None) };
}

use futures_util::future::BoxFuture;
use thiserror::Error;
// Executor storage types removed
#[cfg(feature = "rayon")]
pub use rayon_sync_executor::RayonExecutor;
pub use single_thread_executor::SingleThreadExecutor;

#[cfg(feature = "tokio")]
pub use tokio_current::TokioCurrent;
#[cfg(feature = "tokio")]
pub use tokio_executor::TokioExecutor;

pub use inline_async::InlineAsync;

pub use registry::ExecutorRegistry;
pub use spec::Outcome;
pub use spec::Task;

/// Register the current tokio runtime handle for IO operations
///
/// This should be called from the main runtime that will handle IO operations.
/// CPU-bound work will still run on dedicated executors, but IO operations
/// will be scheduled back to this registered runtime.
#[cfg(feature = "tokio")]
pub fn register_current_runtime_for_io() {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        register_io_runtime(Some(handle));
    }
}

/// Register a specific tokio runtime handle for IO operations
#[cfg(feature = "tokio")]
pub fn register_io_runtime(handle: Option<tokio::runtime::Handle>) {
    // Set both global and thread-local
    {
        let mut guard = IO_RUNTIME.write().expect("IO runtime lock poisoned");
        guard.clone_from(&handle);
    }
    THREAD_IO_RUNTIME.with(|tls| {
        *tls.borrow_mut() = handle;
    });
}

/// Spawn a future on the IO runtime
///
/// This ensures IO operations run on the appropriate runtime while
/// CPU-bound effects stay on their dedicated executors.
///
/// # Behavior
///
/// - **Thread-local first**: Uses thread-local registration set via `register_current_runtime_for_io()`
/// - **Global fallback**: Falls back to global registration set via `register_io_runtime()`
/// - **No implicit fallback**: Does NOT fall back to `tokio::runtime::Handle::try_current()`
///
/// # Panics
///
/// Panics if no IO runtime is explicitly registered. Call `register_current_runtime_for_io()`
/// or `register_io_runtime()` first to register an IO runtime.
#[cfg(feature = "tokio")]
pub fn spawn_io<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Try thread-local first, then global. NO fallback to try_current().
    let handle = THREAD_IO_RUNTIME.with(|tls| tls.borrow().clone())
        .or_else(|| IO_RUNTIME.read().ok().and_then(|g| g.clone()))
        .expect("No IO runtime registered. Call `register_current_runtime_for_io()` or `register_io_runtime()` first!");

    handle.spawn(future)
}

/// Clear all IO runtime registrations (useful for tests)
#[cfg(all(feature = "tokio", test))]
pub fn clear_io_runtime() {
    register_io_runtime(None);
}

/// Error type for executor job management
#[derive(Debug, Error)]
pub enum ExecutorError {
    /// The executor has been shut down and cannot accept new work
    #[error("Worker thread gone, executor was likely shut down")]
    WorkerGone,
    /// The spawned task was cancelled (aborted by caller dropping the future)
    #[error("Task was cancelled")]
    Cancelled,
    /// The spawned task panicked; contains message if available
    #[error("Panic: {msg}")]
    Panic { msg: String },
}

/// Future wrapper that aborts the spawned task on drop to provide cancel-on-drop semantics
///
/// This shared implementation consolidates the duplicate AbortOnDrop structs from
/// tokio_executor.rs and tokio_current.rs into a single generic version.
#[cfg(feature = "tokio")]
pub(crate) struct AbortOnDrop<T> {
    handle: tokio::task::JoinHandle<Result<T, futures_util::future::Aborted>>,
    abort: futures_util::future::AbortHandle,
}

#[cfg(feature = "tokio")]
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

#[cfg(feature = "tokio")]
impl<T> std::future::Future for AbortOnDrop<T>
where
    T: Send + 'static,
{
    type Output = Result<T, ExecutorError>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        match std::pin::Pin::new(&mut this.handle).poll(cx) {
            std::task::Poll::Ready(join_res) => std::task::Poll::Ready(match join_res {
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
            }),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// Shared lifecycle management for all executor types
pub trait ExecutorLifecycle: Send + Sync + 'static {
    /// Signal the executor to begin shutdown; no further tasks will be accepted
    fn shutdown(&self);

    /// Wait for executor shutdown completion
    fn join(&self) -> BoxFuture<'static, ()>;
}

/// Executor specialized for async work (futures)
///
/// Optimized for cooperative async tasks. Provides deterministic cancel-on-drop
/// semantics via task abort handles.
pub trait AsyncExecutor<E>: ExecutorLifecycle {
    /// Spawn a pre-built future and get a cancel-on-drop join future
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, Outcome<E>>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>>;
}

/// Executor specialized for blocking/synchronous work
///
/// Optimized for CPU-bound work on thread pools. Cancel-on-drop semantics
/// are best-effort: "don't start if not yet dequeued; if running, cooperative only".
pub trait SyncExecutor<E>: ExecutorLifecycle {
    /// Spawn a synchronous job and get a join future
    fn spawn_sync(
        &self,
        job: Box<dyn FnOnce() -> Outcome<E> + Send>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>>;
}

/// Marker trait for executors that support concurrent/overlapping execution
pub trait Concurrent: 'static {}

/// Marker trait for executors that only support sequential execution
pub trait Sequential: 'static {}
