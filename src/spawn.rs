//! Zero-cost spawn adapters for different async runtimes
//!
//! Syzygy takes a "tokio-first with runtime flexibility" approach. With Rust 1.85's
//! stable async closures and `AsyncFn` traits, we now provide zero-cost abstractions
//! without boxing overhead.
//!
//! # Usage
//!
//! ## Auto-detection (Recommended)
//!
//! ```rust,ignore
//! // This example shows usage within an application context
//! use syzygy::spawn::auto_spawn_fn;
//!
//! // Zero-cost - no boxing!
//! runner.run_until(condition, auto_spawn_fn()).await?;
//! ```
//!
//! ## Async Closures (Zero-Cost)
//!
//! The preferred way to use spawn functions with Rust 1.85+ async closures:
//!
//! ```rust
//! use syzygy::spawn::auto_spawn;
//!
//! # tokio_test::block_on(async {
//! // Direct async closure - zero overhead
//! auto_spawn(async {
//!     println!("This runs with zero boxing overhead!");
//! });
//! # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//! # });
//! ```
//!
//! ## Generic AsyncFn Support
//!
//! All spawn functions accept any `impl AsyncFnOnce() -> ()`:
//!
//! ```rust
//! use syzygy::spawn::auto_spawn;
//!
//! # tokio_test::block_on(async {
//! async fn my_async_work() {
//!     println!("This is an async function");
//! }
//!
//! auto_spawn(my_async_work());  // Zero-cost function pointer
//! auto_spawn(async {       // Zero-cost async block
//!     println!("Also zero-cost!");
//! });
//! # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//! # });
//! ```
//!
//! ## Legacy BoxFuture Support (Deprecated)
//!
//! For backward compatibility only - prefer AsyncFn for performance:
//!
//! ```rust
//! use syzygy::spawn::auto_spawn_fn;
//!
//! # tokio_test::block_on(async {
//! let spawn_fn = auto_spawn_fn();
//! let future = Box::pin(async { println!("Legacy style - has allocation"); });
//! spawn_fn(future);  // Use _boxed variants for legacy code
//! # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
//! # });
//! ```

use std::future::Future;
use std::pin::Pin;

/// Modern Spawn trait using AFIT for zero-cost abstractions
///
/// This trait provides a clean interface for spawning futures across different
/// async runtimes without boxing overhead.
pub trait Spawn: Clone + Send + Sync + 'static {
    /// Spawn a future on the runtime
    ///
    /// This method accepts any future that outputs `()` without requiring boxing.
    /// The implementation handles the actual spawning mechanism for the specific runtime.
    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static);
}

/// Legacy spawn function signature for backward compatibility
///
/// Use this with auto_spawn_boxed() for the old BoxFuture-based API.
pub type BoxedSpawnFn = dyn Fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync;

/// Spawn a future using tokio (zero-cost)
///
/// This is the primary/recommended spawn function. Tokio dominates the Rust async
/// ecosystem and most users expect it to work out of the box.
///
/// Accepts any future that outputs `()` - no boxing required!
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "tokio")]
/// # {
/// use syzygy::spawn::spawn_tokio;
///
/// # tokio_test::block_on(async {
/// // Async closure
/// spawn_tokio(async {
///     println!("Zero-cost async closure on tokio!");
/// });
///
/// // Async function call
/// async fn my_work() { println!("Also zero-cost!"); }
/// spawn_tokio(my_work());
/// # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// # });
/// # }
/// ```
#[cfg(feature = "tokio")]
pub fn spawn_tokio<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

/// Spawn a future using smol (zero-cost)
///
/// Smol is a lightweight alternative to tokio, suitable for applications
/// that need a smaller runtime footprint.
///
/// Accepts any future that outputs `()` - no boxing required!
#[cfg(feature = "smol")]
pub fn spawn_smol<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    smol::spawn(future).detach();
}

/// Spawn a future using async-std (zero-cost)
///
/// Async-std follows the standard library approach and provides async
/// versions of std library functionality.
///
/// Accepts any future that outputs `()` - no boxing required!
#[cfg(feature = "async-std")]
pub fn spawn_async_std<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    async_std::task::spawn(future);
}

/// Auto-detect and spawn a future directly (zero-cost)
///
/// Uses priority order: tokio > smol > async-std when multiple features are enabled.
/// This is the most ergonomic API for direct spawning with zero boxing overhead.
///
/// # Panics
///
/// Panics at compile time if no async runtime feature is enabled.
///
/// # Example
///
/// ```rust
/// use syzygy::spawn::auto_spawn;
///
/// # tokio_test::block_on(async {
/// // Zero-cost async block spawning - no boxing!
/// auto_spawn(async {
///     println!("This runs on the auto-detected runtime!");
/// });
///
/// // Also works with async function calls - no boxing!
/// async fn my_work() { println!("Zero-cost!"); }
/// auto_spawn(my_work());
/// # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// # });
/// ```
pub fn auto_spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    #[cfg(feature = "tokio")]
    return spawn_tokio(future);

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return spawn_smol(future);

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return spawn_async_std(future);

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

/// Auto-detect and return a Spawn implementation (RPIT)
///
/// This returns an opaque type implementing Spawn for the auto-detected runtime.
/// Uses priority order: tokio > smol > async-std when multiple features are enabled.
///
/// # Example
///
/// ```rust,ignore
/// use syzygy::spawn::auto_spawner;
///
/// // Zero-cost spawner for Runner
/// let spawner = spawner();
/// runner.run_until(condition, spawner).await?;
/// ```
#[must_use]
pub fn spawner() -> impl Spawn {
    #[cfg(feature = "tokio")]
    return TokioSpawn;

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return SmolSpawn;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return AsyncStdSpawn;

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

/// Auto-detect and return spawn function for use with Runner (legacy compatibility)
///
/// This returns a function pointer for use with APIs that expect a spawner function.
/// Prefer `spawner()` for zero-cost abstractions.
///
/// # Example
///
/// ```rust
/// use syzygy::spawn::auto_spawn_fn;
///
/// // For use with legacy APIs that expect a function pointer
/// let spawn_fn = auto_spawn_fn();
/// // runner.run_until(condition, spawn_fn).await?; // Usage in app context
/// ```
pub fn auto_spawn_fn()
-> impl Fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Clone + Send + Sync + 'static {
    #[cfg(feature = "tokio")]
    return spawn_tokio_boxed;

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return spawn_smol_boxed;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return spawn_async_std_boxed;

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

// =============================================================================
// SPAWN TRAIT IMPLEMENTATIONS
// =============================================================================
//
// Concrete implementations of the Spawn trait for each runtime.
// These provide zero-cost abstractions for spawning futures.

/// Tokio runtime spawner
#[derive(Clone, Debug)]
pub struct TokioSpawn;

#[cfg(feature = "tokio")]
impl Spawn for TokioSpawn {
    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        tokio::spawn(future);
    }
}

/// Smol runtime spawner
#[derive(Clone, Debug)]
pub struct SmolSpawn;

#[cfg(feature = "smol")]
impl Spawn for SmolSpawn {
    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        smol::spawn(future).detach();
    }
}

/// Async-std runtime spawner
#[derive(Clone, Debug)]
pub struct AsyncStdSpawn;

#[cfg(feature = "async-std")]
impl Spawn for AsyncStdSpawn {
    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        async_std::task::spawn(future);
    }
}

// =============================================================================
// BACKWARD COMPATIBILITY - BOXED FUTURE SUPPORT (DEPRECATED)
// =============================================================================
//
// These functions provide backward compatibility with the old BoxFuture-based API.
// They have allocation overhead and should be avoided in new code.
// Prefer the Spawn trait implementations above for zero-cost abstractions.

/// Legacy spawn function for tokio using boxed futures (DEPRECATED)
///
/// This function has allocation overhead due to boxing. Prefer `spawn_tokio` for
/// zero-cost abstractions with async closures.
#[cfg(feature = "tokio")]
pub fn spawn_tokio_boxed(future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
    tokio::spawn(future);
}

/// Legacy spawn function for smol using boxed futures (DEPRECATED)
///
/// This function has allocation overhead due to boxing. Prefer `spawn_smol` for
/// zero-cost abstractions with async closures.
#[cfg(feature = "smol")]
pub fn spawn_smol_boxed(future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
    smol::spawn(future).detach();
}

/// Legacy spawn function for async-std using boxed futures (DEPRECATED)
///
/// This function has allocation overhead due to boxing. Prefer `spawn_async_std` for
/// zero-cost abstractions with async closures.
#[cfg(feature = "async-std")]
pub fn spawn_async_std_boxed(future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
    async_std::task::spawn(future);
}

/// Legacy auto-spawn function returning a function pointer (DEPRECATED)
///
/// This API has allocation overhead and requires boxing futures.
/// Prefer the direct `auto_spawn(async_closure)` API for zero-cost abstractions.
///
/// # Example (old API - avoid in new code)
///
/// ```rust
/// use syzygy::spawn::auto_spawn_boxed;
///
/// # tokio_test::block_on(async {
/// let spawn_fn = auto_spawn_boxed();
/// spawn_fn(Box::pin(async { println!("Legacy boxed future"); }));
/// # tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// # });
/// ```
pub fn auto_spawn_boxed()
-> impl Fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Clone + Send + Sync + 'static {
    #[cfg(feature = "tokio")]
    return spawn_tokio_boxed;

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return spawn_smol_boxed;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return spawn_async_std_boxed;

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
    async fn test_auto_spawn_actually_works() {
        use std::sync::{Arc, Mutex};

        // This test actually spawns an async block and verifies it executes
        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        // Zero-cost async block spawning
        auto_spawn(async move {
            *executed_clone.lock().unwrap() = true;
        });

        // Give the spawned task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed.lock().unwrap(),
            "Spawned async block should have executed"
        );
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_async_spawn_compiles() {
        // Test that the async spawn functions compile with different future types

        // Test async block
        spawn_tokio(async {
            println!("Test async block");
        });

        // Test async function call
        #[allow(clippy::unused_async)]
        #[allow(clippy::items_after_statements)] // Test function - local scope is fine
        async fn test_async_fn() {
            println!("Test async function");
        }

        spawn_tokio(test_async_fn());
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_tokio_spawn_compiles() {
        // Test that tokio spawn accepts different types of futures
        spawn_tokio(async {
            // Async block test
        });

        #[allow(clippy::unused_async)]
        #[allow(clippy::items_after_statements)] // Test function - local scope is fine
        async fn test_fn() {}
        spawn_tokio(test_fn());
    }

    #[cfg(feature = "smol")]
    #[test]
    fn test_smol_spawn_compiles() {
        // Just test that the function can be referenced
        let _fn_ref = spawn_smol::<std::future::Ready<()>>;
    }

    #[cfg(feature = "async-std")]
    #[test]
    fn test_async_std_spawn_compiles() {
        // Just test that the function can be referenced
        let _fn_ref = spawn_async_std::<std::future::Ready<()>>;
    }

    /// Test Spawn trait implementations
    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_spawn_trait() {
        use std::sync::{Arc, Mutex};

        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        // Test TokioSpawn
        let tokio_spawner = TokioSpawn;
        tokio_spawner.spawn(async move {
            *executed_clone.lock().unwrap() = true;
        });

        // Give the spawned task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(*executed.lock().unwrap(), "TokioSpawn should execute tasks");

        // Test auto_spawner
        let executed2 = Arc::new(Mutex::new(false));
        let executed2_clone = Arc::clone(&executed2);

        let spawnr = spawner();
        spawnr.spawn(async move {
            *executed2_clone.lock().unwrap() = true;
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed2.lock().unwrap(),
            "auto_spawner should execute tasks"
        );
    }

    /// Test zero-cost async spawn functions work correctly
    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_zero_cost_async_spawn() {
        use std::sync::{Arc, Mutex};

        #[allow(clippy::unused_async, dead_code)]
        async fn test_async_function(flag: Arc<Mutex<bool>>) {
            *flag.lock().unwrap() = true;
        }

        // Test direct spawn_tokio with async block
        let executed = Arc::new(Mutex::new(false));
        let executed_clone = Arc::clone(&executed);

        spawn_tokio(async move {
            *executed_clone.lock().unwrap() = true;
        });

        // Give the spawned task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed.lock().unwrap(),
            "Async block should have executed via spawn_tokio"
        );

        // Test auto_spawn with async block
        let executed2 = Arc::new(Mutex::new(false));
        let executed2_clone = Arc::clone(&executed2);

        auto_spawn(async move {
            *executed2_clone.lock().unwrap() = true;
        });

        // Give the spawned task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed2.lock().unwrap(),
            "Async block should have executed via auto_spawn"
        );

        // Test with async function call
        let executed3 = Arc::new(Mutex::new(false));
        let executed3_clone = Arc::clone(&executed3);

        spawn_tokio(test_async_function(executed3_clone));

        // Give the spawned task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed3.lock().unwrap(),
            "Async function should have executed via spawn_tokio"
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
        let spawn_fn = auto_spawn_fn();
        let future = Box::pin(async move {
            *executed_clone.lock().unwrap() = true;
        });

        spawn_fn(future);

        // Give the spawned task time to execute
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert!(
            *executed.lock().unwrap(),
            "Boxed future should have executed"
        );
    }
}
