//! Pluggable async runtime abstractions used by shell execution.

#[cfg(all(feature = "rt-compio", feature = "rt-tokio"))]
compile_error!("features `rt-compio` and `rt-tokio` are mutually exclusive");

#[cfg(not(any(feature = "rt-compio", feature = "rt-tokio")))]
compile_error!("a runtime feature must be enabled: `rt-compio` or `rt-tokio`");

#[cfg(feature = "rt-compio")]
mod imp {
    use std::future::{poll_fn, Future};
    use std::task::Poll;
    use std::time::Duration;

    pub type JoinHandle<T> = compio::runtime::JoinHandle<T>;

    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
    {
        compio::runtime::spawn(future)
    }

    pub fn spawn_blocking<F, T>(blocking: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        compio::runtime::spawn_blocking(blocking)
    }

    pub async fn await_blocking<T>(handle: JoinHandle<T>) -> T {
        match handle.await {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
        compio::runtime::time::sleep(duration)
    }

    pub async fn yield_now() {
        // compio does not expose a first-class yield primitive. A zero-duration
        // timer completes immediately, so explicitly self-wake once to
        // requeue this task behind other ready tasks.
        let mut yielded = false;
        poll_fn(move |cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
    }

    #[allow(clippy::result_unit_err)]
    pub fn try_with_current<F, R>(f: F) -> Result<R, ()>
    where
        F: FnOnce() -> R,
    {
        compio::runtime::Runtime::try_with_current(|_| f()).map_err(|_| ())
    }

    #[must_use]
    pub fn supports_sync_driving() -> bool {
        true
    }

    #[must_use]
    pub fn run() -> bool {
        compio::runtime::Runtime::try_with_current(compio::runtime::Runtime::run).unwrap_or(false)
    }

    pub fn poll_with(duration: Option<Duration>) {
        let _ = compio::runtime::Runtime::try_with_current(|runtime| runtime.poll_with(duration));
    }

    pub fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        compio::runtime::Runtime::new()
            .expect("failed to create compio runtime")
            .block_on(future)
    }
}

#[cfg(feature = "rt-tokio")]
mod imp {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

    #[derive(Debug)]
    struct TaskCancelled;

    #[derive(Debug)]
    pub struct JoinHandle<T> {
        inner: Option<tokio::task::JoinHandle<T>>,
    }

    impl<T> Drop for JoinHandle<T> {
        fn drop(&mut self) {
            if let Some(handle) = self.inner.take() {
                handle.abort();
            }
        }
    }

    impl<T> Future for JoinHandle<T> {
        type Output = Result<T, Box<dyn std::any::Any + Send + 'static>>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let handle = self
                .inner
                .as_mut()
                .expect("tokio join handle polled after completion");

            match Pin::new(handle).poll(cx) {
                Poll::Ready(Ok(result)) => Poll::Ready(Ok(result)),
                Poll::Ready(Err(err)) => {
                    if err.is_panic() {
                        Poll::Ready(Err(err.into_panic()))
                    } else {
                        Poll::Ready(Err(Box::new(TaskCancelled)))
                    }
                }
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl<T> JoinHandle<T> {
        #[must_use]
        pub fn is_finished(&self) -> bool {
            self.inner
                .as_ref()
                .map_or(true, tokio::task::JoinHandle::is_finished)
        }
    }

    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        JoinHandle {
            inner: Some(tokio::task::spawn_local(future)),
        }
    }

    pub fn spawn_blocking<F, T>(blocking: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        JoinHandle {
            inner: Some(tokio::task::spawn_blocking(blocking)),
        }
    }

    pub async fn await_blocking<T>(handle: JoinHandle<T>) -> T {
        match handle.await {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
        tokio::time::sleep(duration)
    }

    pub async fn yield_now() {
        tokio::task::yield_now().await;
    }

    #[allow(clippy::result_unit_err)]
    pub fn try_with_current<F, R>(f: F) -> Result<R, ()>
    where
        F: FnOnce() -> R,
    {
        if tokio::runtime::Handle::try_current().is_ok() {
            Ok(f())
        } else {
            Err(())
        }
    }

    #[must_use]
    pub fn supports_sync_driving() -> bool {
        false
    }

    #[must_use]
    pub fn run() -> bool {
        false
    }

    pub fn poll_with(_duration: Option<Duration>) {}

    pub fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");
        let local_set = tokio::task::LocalSet::new();
        local_set.block_on(&runtime, future)
    }
}

pub use imp::*;
