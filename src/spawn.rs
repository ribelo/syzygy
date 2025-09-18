use std::future::Future;

/// Trait representing the ability to spawn a future onto an async executor.
pub trait Spawn: Clone + Send + Sync + 'static {
    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static;
}

#[cfg(feature = "tokio")]
#[derive(Clone)]
pub struct TokioSpawn {
    handle: tokio::runtime::Handle,
}

#[cfg(feature = "tokio")]
impl TokioSpawn {
    fn new() -> Self {
        let handle = tokio::runtime::Handle::try_current().expect(
            "No tokio runtime available. Call Syzygy builder with_async_executor(...) or run inside #[tokio::main].",
        );
        Self { handle }
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

/// Return the active async spawner.
///
/// This helper requires a registered async executor. If no executor/runtime is
/// available, it fails fast instead of silently falling back to inline execution.
#[cfg(feature = "tokio")]
#[must_use]
pub fn spawner() -> TokioSpawn {
    TokioSpawn::new()
}

#[cfg(not(feature = "tokio"))]
pub fn spawner() -> ! {
    panic!(
        "No async runtime feature enabled. Enable the `tokio` feature or provide your own executor."
    );
}
