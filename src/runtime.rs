use std::future::Future;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

#[cfg(feature = "smol")]
use futures::future::{Either, select};

use futures::future::BoxFuture;

#[cfg(feature = "tokio")]
use tokio::runtime::Handle;

#[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
compile_error!(
    "syzygy requires at least one async runtime feature: enable 'tokio', 'smol', or 'async-std'"
);

pub type SleepFuture = BoxFuture<'static, ()>;

pub type TimeoutResult<T> = Result<T, TimeoutError>;

pub type TimeoutFuture<T> = BoxFuture<'static, TimeoutResult<T>>;

#[derive(Debug, Clone)]
pub struct Runtime {
    kind: RuntimeKind,
}

#[derive(Debug, Clone)]
enum RuntimeKind {
    #[cfg(feature = "tokio")]
    Tokio(Handle),
    #[cfg(feature = "smol")]
    Smol,
    #[cfg(feature = "async-std")]
    AsyncStd,
    Inline,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RuntimeError {
    #[cfg(feature = "tokio")]
    #[error("No tokio runtime is running. Use #[tokio::main] or create a runtime first.")]
    TokioUnavailable,
    #[error("No async runtime is available for the requested operation.")]
    Unavailable,
}

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

impl Runtime {
    #[must_use]
    pub fn current() -> Self {
        Self::strict().unwrap_or_else(|_| Self::inline())
    }

    pub fn strict() -> Result<Self, RuntimeError> {
        if let Some(kind) = Self::detect_kind() {
            return Ok(Self { kind });
        }

        #[cfg(feature = "tokio")]
        {
            return Err(RuntimeError::TokioUnavailable);
        }

        #[allow(unreachable_code)]
        Err(RuntimeError::Unavailable)
    }

    #[cfg(feature = "tokio")]
    pub fn tokio() -> Result<Self, RuntimeError> {
        Handle::try_current()
            .map(|handle| Self {
                kind: RuntimeKind::Tokio(handle),
            })
            .map_err(|_| RuntimeError::TokioUnavailable)
    }

    #[cfg(feature = "smol")]
    #[must_use]
    pub fn smol() -> Self {
        Self {
            kind: RuntimeKind::Smol,
        }
    }

    #[cfg(feature = "async-std")]
    #[must_use]
    pub fn async_std() -> Self {
        Self {
            kind: RuntimeKind::AsyncStd,
        }
    }

    #[must_use]
    pub fn inline() -> Self {
        Self {
            kind: RuntimeKind::Inline,
        }
    }

    #[must_use]
    pub fn allows_overlap(&self) -> bool {
        !matches!(self.kind, RuntimeKind::Inline)
    }

    pub fn schedule<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match &self.kind {
            #[cfg(feature = "tokio")]
            RuntimeKind::Tokio(handle) => {
                handle.spawn(future);
            }
            #[cfg(feature = "smol")]
            RuntimeKind::Smol => {
                smol::spawn(future).detach();
            }
            #[cfg(feature = "async-std")]
            RuntimeKind::AsyncStd => {
                async_std::task::spawn(future);
            }
            RuntimeKind::Inline => {
                futures::executor::block_on(future);
            }
        }
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.schedule(future);
    }

    #[must_use]
    pub fn sleep(&self, duration: Duration) -> SleepFuture {
        match &self.kind {
            #[cfg(feature = "tokio")]
            RuntimeKind::Tokio(_) => Box::pin(async move {
                tokio::time::sleep(duration).await;
            }),
            #[cfg(feature = "smol")]
            RuntimeKind::Smol => Box::pin(async move {
                smol::Timer::after(duration).await;
            }),
            #[cfg(feature = "async-std")]
            RuntimeKind::AsyncStd => Box::pin(async move {
                async_std::task::sleep(duration).await;
            }),
            RuntimeKind::Inline => Box::pin(async move {
                std::thread::sleep(duration);
            }),
        }
    }

    pub async fn timeout<F, T>(&self, duration: Duration, future: F) -> TimeoutResult<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        match &self.kind {
            #[cfg(feature = "tokio")]
            RuntimeKind::Tokio(_) => match tokio::time::timeout(duration, future).await {
                Ok(value) => Ok(value),
                Err(_) => Err(TimeoutError::new(duration)),
            },
            #[cfg(feature = "smol")]
            RuntimeKind::Smol => {
                let timer = smol::Timer::after(duration);
                futures::pin_mut!(timer);
                futures::pin_mut!(future);
                match select(timer, future).await {
                    Either::Left(_) => Err(TimeoutError::new(duration)),
                    Either::Right((value, _)) => Ok(value),
                }
            }
            #[cfg(feature = "async-std")]
            RuntimeKind::AsyncStd => match async_std::future::timeout(duration, future).await {
                Ok(value) => Ok(value),
                Err(_) => Err(TimeoutError::new(duration)),
            },
            RuntimeKind::Inline => Self::inline_timeout(duration, future).await,
        }
    }

    fn detect_kind() -> Option<RuntimeKind> {
        #[cfg(feature = "tokio")]
        {
            if let Ok(handle) = Handle::try_current() {
                return Some(RuntimeKind::Tokio(handle));
            }
        }

        #[cfg(feature = "smol")]
        {
            return Some(RuntimeKind::Smol);
        }

        #[cfg(feature = "async-std")]
        {
            return Some(RuntimeKind::AsyncStd);
        }

        None
    }

    async fn inline_timeout<F, T>(duration: Duration, future: F) -> TimeoutResult<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = futures::executor::block_on(future);
            let _ = tx.send(result);
        });

        match rx.recv_timeout(duration) {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => Err(TimeoutError::new(duration)),
            Err(RecvTimeoutError::Disconnected) => Err(TimeoutError::new(duration)),
        }
    }
}

impl From<Runtime> for RuntimeKind {
    fn from(runtime: Runtime) -> Self {
        runtime.kind
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::current()
    }
}
