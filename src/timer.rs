//! Timer abstractions for runtime-neutral async operations
//!
//! # Runtime Priority System
//!
//! When multiple async runtime features are enabled simultaneously, Syzygy uses the following priority order:
//!
//! 1. **tokio** - Highest priority (most mature ecosystem)
//! 2. **smol** - Medium priority (lightweight alternative)
//! 3. **async-std** - Lowest priority (standard library alternative)
//!
//! This means if you enable both `tokio` and `smol` features, Syzygy will use tokio.
//!
//! # Usage
//!
//! Enable at least one runtime feature in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! syzygy = { version = "0.1", features = ["tokio"] }
//! # or
//! syzygy = { version = "0.1", features = ["smol"] }
//! # or
//! syzygy = { version = "0.1", features = ["async-std"] }
//! ```
//!
//! The library will automatically detect and use the appropriate runtime implementation.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Type alias for boxed sleep futures
pub type SleepFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Type alias for boxed timeout result futures
pub type TimeoutResult<T> = Result<T, TimeoutError>;
pub type TimeoutFuture<T> = Pin<Box<dyn Future<Output = TimeoutResult<T>> + Send + 'static>>;

/// Runtime implementation selector
#[derive(Debug, Clone, Copy)]
pub enum Time {
    #[cfg(feature = "tokio")]
    Tokio,
    #[cfg(feature = "smol")]
    Smol,
    #[cfg(feature = "async-std")]
    AsyncStd,
}

impl Time {
    /// Create a sleep future for the specified duration
    #[must_use]
    pub fn sleep(self, duration: Duration) -> SleepFuture {
        match self {
            #[cfg(feature = "tokio")]
            Time::Tokio => Box::pin(async move {
                tokio::time::sleep(duration).await;
            }),
            #[cfg(feature = "smol")]
            Time::Smol => Box::pin(async move {
                smol::Timer::after(duration).await;
            }),
            #[cfg(feature = "async-std")]
            Time::AsyncStd => Box::pin(async move {
                async_std::task::sleep(duration).await;
            }),
        }
    }

    /// Create a timeout future for the specified duration and future
    ///
    /// Legacy boxed API kept for compatibility. Prefer the free `timeout` function
    /// for zero-cost generic futures.
    #[must_use]
    pub fn timeout(
        self,
        duration: Duration,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> TimeoutFuture<()> {
        match self {
            #[cfg(feature = "tokio")]
            Time::Tokio => Box::pin(async move {
                match tokio::time::timeout(duration, future).await {
                    Ok(()) => Ok(()),
                    Err(_) => Err(TimeoutError::new(duration)),
                }
            }),
            #[cfg(feature = "smol")]
            Time::Smol => Box::pin(async move {
                // Use futures::select! for timeout implementation
                use futures::future::{Either, select};
                use futures::pin_mut;

                let timer = smol::Timer::after(duration);
                pin_mut!(timer);
                pin_mut!(future);

                match select(timer, future).await {
                    Either::Left(_) => Err(TimeoutError::new(duration)),
                    Either::Right(((), _)) => Ok(()),
                }
            }),
            #[cfg(feature = "async-std")]
            Time::AsyncStd => Box::pin(async move {
                match async_std::future::timeout(duration, future).await {
                    Ok(()) => Ok(()),
                    Err(_) => Err(TimeoutError::new(duration)),
                }
            }),
        }
    }
}

// Type aliases removed - they were confusing and served no purpose
// Use RuntimeImpl directly instead

/// Error type for timeout operations
#[derive(Debug, Clone, thiserror::Error)]
#[error("Operation timed out after {duration:?}")]
pub struct TimeoutError {
    pub duration: Duration,
}

impl TimeoutError {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }
}

// Runtime implementations are now integrated into the RuntimeImpl enum above

/// Auto-detect runtime implementation
///
/// Uses priority order: tokio > smol > async-std when multiple features are enabled.
///
/// # Compile-time requirement
/// At least one of the following features must be enabled: tokio, smol, or async-std
#[must_use]
pub fn time() -> Time {
    #[cfg(feature = "tokio")]
    return Time::Tokio;

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return Time::Smol;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return Time::AsyncStd;

    // Compile-time error if no runtime is available
    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
    );
}

/// Zero-cost timeout for generic futures (no boxing)
///
/// Uses compile-time runtime selection (tokio > smol > async-std).
/// Prefer this over `Time::timeout` when possible.
pub async fn timeout<F, T>(duration: Duration, future: F) -> TimeoutResult<T>
where
    F: Future<Output = T> + Send + 'static,
{
    #[cfg(feature = "tokio")]
    {
        match tokio::time::timeout(duration, future).await {
            Ok(v) => Ok(v),
            Err(_) => Err(TimeoutError::new(duration)),
        }
    }

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    {
        use futures::future::{Either, select};
        futures::pin_mut!(future);
        let timer = smol::Timer::after(duration);
        futures::pin_mut!(timer);
        match select(timer, future).await {
            Either::Left(_) => Err(TimeoutError::new(duration)),
            Either::Right((v, _)) => Ok(v),
        }
    }

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    {
        match async_std::future::timeout(duration, future).await {
            Ok(v) => Ok(v),
            Err(_) => Err(TimeoutError::new(duration)),
        }
    }

    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!(
        "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'",
    );
}

// Deprecated functions removed - use auto_runtime() directly
