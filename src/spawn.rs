//! Runtime-neutral spawn functions for zero-cost task spawning
//!
//! This module provides spawn functions that work with different async runtimes.

use std::future::Future;

/// Spawn trait for runtime abstraction
pub trait Spawn: Clone + Send + Sync + 'static {
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static;
}

/// Auto-detecting spawner (strict - requires existing runtime)
#[must_use]
pub fn spawner() -> impl Spawn {
    #[cfg(feature = "tokio")]
    return TokioSpawn::new()
        .expect("No tokio runtime is running. Use #[tokio::main] or create a runtime first.");

    #[cfg(all(feature = "smol", not(feature = "tokio")))]
    return SmolSpawn;

    #[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
    return AsyncStdSpawn;

    #[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
    compile_error!("Enable at least one runtime feature: tokio, smol, or async-std");
}

/// Tokio-specific spawner (strict)
#[cfg(feature = "tokio")]
#[derive(Clone)]
pub struct TokioSpawn {
    handle: tokio::runtime::Handle,
}

#[cfg(feature = "tokio")]
impl TokioSpawn {
    pub fn new() -> Result<Self, &'static str> {
        tokio::runtime::Handle::try_current()
            .map_err(
                |_| "No tokio runtime is running. Use #[tokio::main] or create a runtime first.",
            )
            .map(|handle| Self { handle })
    }
}

#[cfg(feature = "tokio")]
impl Spawn for TokioSpawn {
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.handle.spawn(future);
    }
}

/// Smol-specific spawner
#[cfg(feature = "smol")]
#[derive(Clone)]
pub struct SmolSpawn;

#[cfg(feature = "smol")]
impl Spawn for SmolSpawn {
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        smol::spawn(future).detach();
    }
}

/// Async-std-specific spawner
#[cfg(feature = "async-std")]
#[derive(Clone)]
pub struct AsyncStdSpawn;

#[cfg(feature = "async-std")]
impl Spawn for AsyncStdSpawn {
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        async_std::task::spawn(future);
    }
}
