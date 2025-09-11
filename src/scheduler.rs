//! Zero-cost schedule adapters for different async runtimes
//!
//! Syzygy takes a "tokio-first with runtime flexibility" approach. With Rust 1.85's
//! stable async closures and `AsyncFn` traits, we now provide zero-cost abstractions
//! without boxing overhead.
//!
//! # Usage
//!
//! ## Auto-detection (Recommended)
//!
//!
//! ## Async Closures (Zero-Cost)
//!
//! The preferred way to use schedule functions with Rust 1.85+ async closures:
//!
//! ```rust
//! use syzygy::scheduler::auto_schedule;
//!
//! # tokio_test::block_on(async {
//! // Direct async closure - zero overhead
//! auto_schedule(async {
//!     println!("This runs with zero boxing overhead!");
//! });
//! # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//! # });
//! ```
//!
//! ## Generic `AsyncFn` Support
//!
//! All schedule functions accept any `impl AsyncFnOnce() -> ()`:
//!
//! ```rust
//! use syzygy::scheduler::auto_schedule;
//!
//! # tokio_test::block_on(async {
//! async fn my_async_work() {
//!     println!("This is an async function");
//! }
//!
//! auto_schedule(my_async_work());  // Zero-cost function pointer
//! auto_schedule(async {       // Zero-cost async block
//!     println!("Also zero-cost!");
//! });
//! # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//! # });
//! ```
//!
//! ## Legacy `BoxFuture` Support (Deprecated)
//!
//! For backward compatibility only - prefer `AsyncFn` for performance:
//!
//! ```rust
//! use syzygy::scheduler::auto_schedule_fn;
//!
//! # tokio_test::block_on(async {
//! let schedule_fn = auto_schedule_fn();
//! let future = Box::pin(async { println!("Legacy style - has allocation"); });
//! schedule_fn(future);  // Use _boxed variants for legacy code
//! # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//! # });
//! ```

use std::future::Future;
use std::pin::Pin;

/// Modern Scheduler trait using AFIT for zero-cost abstractions
///
/// This trait provides a clean interface for scheduling futures across different
/// async runtimes without boxing overhead.
pub trait Scheduler: Clone + Send + Sync + 'static {
    /// Schedule a future on the runtime
    ///
    /// This method accepts any future that outputs `()` without requiring boxing.
    /// The implementation handles the actual scheduling mechanism for the specific runtime.
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static);
}

/// Legacy schedule function signature for backward compatibility
///
/// Use this with `auto_schedule_boxed()` for the old BoxFuture-based API.
pub type BoxedScheduleFn = dyn Fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync;



/// Schedule a future using tokio (zero-cost)
///
/// This is the primary/recommended schedule function. Tokio dominates the Rust async
/// ecosystem and most users expect it to work out of the box.
///
/// Accepts any future that outputs `()` - no boxing required!
///
/// # Panics
///
/// Panics if no tokio runtime is running. Use `TokioScheduler::new()` for error handling.
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "tokio")]
/// # {
/// use syzygy::scheduler::schedule_tokio;
///
/// # tokio_test::block_on(async {
/// // Async closure
/// schedule_tokio(async {
///     println!("Zero-cost async closure on tokio!");
/// });
///
/// // Async function call
/// async fn my_work() { println!("Also zero-cost!"); }
/// schedule_tokio(my_work());
/// # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// # });
/// # }
/// ```
#[cfg(feature = "tokio")]
pub fn schedule_tokio<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    TokioScheduler::new()
        .expect("No tokio runtime is running. Use #[tokio::main] or create a runtime first.")
        .schedule(future);
}

/// Schedule a future using smol (zero-cost)
///
/// Smol is a lightweight alternative to tokio, suitable for applications
/// that need a smaller runtime footprint.
///
/// Accepts any future that outputs `()` - no boxing required!
#[cfg(feature = "smol")]
pub fn schedule_smol<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    smol::spawn(future).detach();
}

/// Schedule a future using async-std (zero-cost)
///
/// Async-std follows the standard library approach and provides async
/// versions of std library functionality.
///
/// Accepts any future that outputs `()` - no boxing required!
#[cfg(feature = "async-std")]
pub fn schedule_async_std<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    async_std::task::spawn(future);
}

/// Auto-detect and schedule a future directly (zero-cost)
///
/// Uses priority order: tokio > smol > async-std when multiple features are enabled.
/// This is the most ergonomic API for direct scheduling with zero boxing overhead.
///
/// # Panics
///
/// Panics at compile time if no async runtime feature is enabled.
///
/// # Example
///
/// ```rust
/// use syzygy::scheduler::auto_schedule;
///
/// # tokio_test::block_on(async {
/// // Zero-cost async block scheduling - no boxing!
/// auto_schedule(async {
///     println!("This runs on the auto-detected runtime!");
/// });
///
/// // Also works with async function calls - no boxing!
/// async fn my_work() { println!("Zero-cost!"); }
/// auto_schedule(my_work());
/// # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// # });
/// ```
pub fn auto_schedule<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    #[cfg(feature = "tokio")]
    return schedule_tokio(future);

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return schedule_smol(future);

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return schedule_async_std(future);

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

/// Auto-detect and return a Scheduler implementation (RPIT)
///
/// This returns an opaque type implementing Scheduler for the auto-detected runtime.
/// Uses priority order: tokio > smol > async-std when multiple features are enabled.
///
/// # Panics
///
/// Panics if no tokio runtime is running (when tokio feature is enabled).
/// Use `scheduler_strict()` for error handling.
///
/// # Example
///
/// ```rust
/// use syzygy::scheduler::scheduler;
///
/// // Zero-cost scheduler for Runner
/// let scheduler = scheduler();
/// // Use with runner: runner.run_until(condition, scheduler).await?;
/// ```
#[must_use]
pub fn scheduler() -> impl Scheduler {
    #[cfg(feature = "tokio")]
    return TokioScheduler::new().expect("No tokio runtime available");

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return SmolScheduler;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return AsyncStdScheduler;

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

/// Auto-detect and return a Scheduler implementation with error handling
///
/// This returns an opaque type implementing Scheduler for the auto-detected runtime.
/// Uses priority order: tokio > smol > async-std when multiple features are enabled.
///
/// # Example
///
/// ```rust
/// use syzygy::scheduler::scheduler_strict;
///
/// // Zero-cost scheduler for Runner with error handling
/// match scheduler_strict() {
///     Ok(scheduler) => {
///         // Use with runner: runner.run_until(condition, scheduler).await?;
///     }
///     Err(e) => println!("No runtime available: {}", e),
/// }
/// ```
#[must_use]
pub fn scheduler_strict() -> Result<impl Scheduler, &'static str> {
    #[cfg(feature = "tokio")]
    return TokioScheduler::new();

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return Ok(SmolScheduler);

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return Ok(AsyncStdScheduler);

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}



/// Auto-detect and return schedule function for use with Runner (legacy compatibility)
///
/// This returns a function pointer for use with APIs that expect a scheduler function.
/// Prefer `scheduler()` for zero-cost abstractions.
///
/// # Example
///
/// ```rust
/// use syzygy::scheduler::auto_schedule_fn;
///
/// // For use with legacy APIs that expect a function pointer
/// let schedule_fn = auto_schedule_fn();
/// // runner.run_until(condition, schedule_fn).await?; // Usage in app context
/// ```
pub fn auto_schedule_fn()
-> impl Fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Clone + Send + Sync + 'static {
    #[cfg(feature = "tokio")]
    return schedule_tokio_boxed;

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return schedule_smol_boxed;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return schedule_async_std_boxed;

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

// =============================================================================
// SCHEDULER TRAIT IMPLEMENTATIONS
// =============================================================================
//
// Concrete implementations of the Scheduler trait for each runtime.
// These provide zero-cost abstractions for scheduling futures.

/// Tokio runtime scheduler (strict - uses current runtime only)
#[derive(Clone)]
pub struct TokioScheduler {
    handle: tokio::runtime::Handle,
}

impl TokioScheduler {
    pub fn new() -> Result<Self, &'static str> {
        tokio::runtime::Handle::try_current()
            .map_err(|_| "No tokio runtime is running. Use #[tokio::main] or create a runtime first.")
            .map(|handle| Self { handle })
    }
}

#[cfg(feature = "tokio")]
impl Scheduler for TokioScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.handle.spawn(future);
    }
}

/// Smol runtime scheduler
#[derive(Clone, Debug)]
pub struct SmolScheduler;

#[cfg(feature = "smol")]
impl Scheduler for SmolScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        smol::spawn(future).detach();
    }
}

/// Async-std runtime scheduler
#[derive(Clone, Debug)]
pub struct AsyncStdScheduler;

#[cfg(feature = "async-std")]
impl Scheduler for AsyncStdScheduler {
    fn schedule(&self, future: impl Future<Output = ()> + Send + 'static) {
        async_std::task::spawn(future);
    }
}

// =============================================================================
// BACKWARD COMPATIBILITY - BOXED FUTURE SUPPORT (DEPRECATED)
// =============================================================================
//
// These functions provide backward compatibility with the old BoxFuture-based API.
// They have allocation overhead and should be avoided in new code.
// Prefer the Scheduler trait implementations above for zero-cost abstractions.

/// Legacy schedule function for tokio using boxed futures (DEPRECATED)
///
/// This function has allocation overhead due to boxing. Prefer `schedule_tokio` for
/// zero-cost abstractions with async closures.
#[cfg(feature = "tokio")]
pub fn schedule_tokio_boxed(future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
    TokioScheduler::new()
        .expect("No tokio runtime is running. Use #[tokio::main] or create a runtime first.")
        .schedule(future);
}

/// Legacy schedule function for smol using boxed futures (DEPRECATED)
///
/// This function has allocation overhead due to boxing. Prefer `schedule_smol` for
/// zero-cost abstractions with async closures.
#[cfg(feature = "smol")]
pub fn schedule_smol_boxed(future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
    smol::spawn(future).detach();
}

/// Legacy schedule function for async-std using boxed futures (DEPRECATED)
///
/// This function has allocation overhead due to boxing. Prefer `schedule_async_std` for
/// zero-cost abstractions with async closures.
#[cfg(feature = "async-std")]
pub fn schedule_async_std_boxed(future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
    async_std::task::spawn(future);
}

/// Legacy auto-schedule function returning a function pointer (DEPRECATED)
///
/// This API has allocation overhead and requires boxing futures.
/// Prefer the direct `auto_schedule(async_closure)` API for zero-cost abstractions.
///
/// # Example (old API - avoid in new code)
///
/// ```rust
/// use syzygy::scheduler::auto_schedule_boxed;
///
/// # tokio_test::block_on(async {
/// let schedule_fn = auto_schedule_boxed();
/// schedule_fn(Box::pin(async { println!("Legacy boxed future"); }));
/// # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// # });
/// ```
pub fn auto_schedule_boxed()
-> impl Fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Clone + Send + Sync + 'static {
    #[cfg(feature = "tokio")]
    return schedule_tokio_boxed;

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return schedule_smol_boxed;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return schedule_async_std_boxed;

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_auto_schedule_actually_works() {
        use std::sync::{Arc, Mutex};

        // This test actually schedules an async block and verifies it executes
        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        // Zero-cost async block scheduling
        auto_schedule(async move {
            *executed_clone.lock().unwrap() = true;
        });

        // Give the scheduled task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed.lock().unwrap(),
            "Scheduled async block should have executed"
        );
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_async_schedule_compiles() {
        // Test that the async schedule functions compile with different future types

        // Test async block
        schedule_tokio(async {
            println!("Test async block");
        });

        // Test async function call
        #[allow(clippy::unused_async)]
        #[allow(clippy::items_after_statements)] // Test function - local scope is fine
        async fn test_async_fn() {
            println!("Test async function");
        }

        schedule_tokio(test_async_fn());
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_tokio_schedule_compiles() {
        // Test that tokio schedule accepts different types of futures
        schedule_tokio(async {
            // Async block test
        });

        #[allow(clippy::unused_async)]
        #[allow(clippy::items_after_statements)] // Test function - local scope is fine
        async fn test_fn() {}
        schedule_tokio(test_fn());
    }

    #[cfg(feature = "smol")]
    #[test]
    fn test_smol_schedule_compiles() {
        // Just test that the function can be referenced
        let _ = schedule_smol::<std::future::Ready<()>>;
    }

    #[cfg(feature = "async-std")]
    #[test]
    fn test_async_std_schedule_compiles() {
        // Just test that the function can be referenced
        let _ = schedule_async_std::<std::future::Ready<()>>;
    }

    /// Test Scheduler trait implementations
    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_scheduler_trait() {
        use std::sync::{Arc, Mutex};

        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        // Test TokioScheduler
        let tokio_scheduler = TokioScheduler::new()
            .expect("No tokio runtime is running. Use #[tokio::main] or create a runtime first.");
        tokio_scheduler.schedule(async move {
            *executed_clone.lock().unwrap() = true;
        });

        // Give the scheduled task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(*executed.lock().unwrap(), "TokioScheduler should execute tasks");

        // Test auto_scheduler
        let executed2 = Arc::new(Mutex::new(false));
        let executed2_clone = Arc::clone(&executed2);

        let schedulr = scheduler();
        schedulr.schedule(async move {
            *executed2_clone.lock().unwrap() = true;
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed2.lock().unwrap(),
            "auto_scheduler should execute tasks"
        );
    }

    /// Test zero-cost async schedule functions work correctly
    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_zero_cost_async_schedule() {
        use std::sync::{Arc, Mutex};

        #[allow(clippy::unused_async, dead_code)]
        async fn test_async_function(flag: Arc<Mutex<bool>>) {
            *flag.lock().unwrap() = true;
        }

        // Test direct schedule_tokio with async block
        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        schedule_tokio(async move {
            *executed_clone.lock().unwrap() = true;
        });

        // Give the scheduled task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed.lock().unwrap(),
            "Async block should have executed via schedule_tokio"
        );

        // Test auto_schedule with async block
        let executed2 = Arc::new(Mutex::new(false));
        let executed2_clone = Arc::clone(&executed2);

        auto_schedule(async move {
            *executed2_clone.lock().unwrap() = true;
        });

        // Give the scheduled task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed2.lock().unwrap(),
            "Async block should have executed via auto_schedule"
        );

        // Test with async function call
        let executed3 = Arc::new(Mutex::new(false));
        let executed3_clone = Arc::clone(&executed3);

        schedule_tokio(test_async_function(executed3_clone));

        // Give the scheduled task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed3.lock().unwrap(),
            "Async function should have executed via schedule_tokio"
        );
    }

    /// Test backward compatibility with boxed futures
    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_boxed_compatibility() {
        use std::sync::{Arc, Mutex};

        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        // Test legacy boxed API
        let schedule_fn = auto_schedule_fn();
        let future = Box::pin(async move {
            *executed_clone.lock().unwrap() = true;
        });

        schedule_fn(future);

        // Give the scheduled task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed.lock().unwrap(),
            "Boxed future should have executed"
        );
    }
}