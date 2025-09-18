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
    fn try_new() -> Result<Self, std::io::Error> {
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No tokio runtime available. Call Syzygy builder with_async_executor(...) or run inside #[tokio::main].",
            ))?;
        Ok(Self { handle })
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
/// available, it returns an error instead of panicking.
#[cfg(feature = "tokio")]
pub fn spawner() -> Result<TokioSpawn, std::io::Error> {
    TokioSpawn::try_new()
}

#[cfg(not(feature = "tokio"))]
pub fn spawner() -> Result<TokioSpawn, std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "No async runtime feature enabled. Enable the `tokio` feature or provide your own executor.",
    ))
}
