//! Owned async runtime abstractions used by shell execution.

#[cfg(all(feature = "rt-compio", feature = "rt-tokio"))]
compile_error!("features `rt-compio` and `rt-tokio` are mutually exclusive");

#[cfg(not(any(feature = "rt-compio", feature = "rt-tokio")))]
compile_error!("a runtime feature must be enabled: `rt-compio` or `rt-tokio`");

#[cfg(feature = "rt-compio")]
mod imp {
    use std::any::Any;
    use std::future::{poll_fn, Future};
    use std::io;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::{Context, Poll};
    use std::time::Duration;

    #[derive(Clone)]
    pub struct Runtime {
        inner: Rc<compio::runtime::Runtime>,
    }

    pub struct JoinHandle<T> {
        inner: Option<compio::runtime::JoinHandle<T>>,
    }

    impl Runtime {
        pub fn new() -> io::Result<Self> {
            Ok(Self {
                inner: Rc::new(compio::runtime::Runtime::new()?),
            })
        }

        #[must_use]
        pub fn ptr_eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.inner, &other.inner)
        }

        pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
        where
            F: Future + 'static,
        {
            JoinHandle {
                inner: Some(self.inner.spawn(future)),
            }
        }

        pub fn spawn_blocking<F, T>(&self, work: F) -> JoinHandle<T>
        where
            F: FnOnce() -> T + Send + 'static,
            T: Send + 'static,
        {
            JoinHandle {
                inner: Some(self.inner.spawn_blocking(work)),
            }
        }

        pub fn drive_ready(&self) {
            self.inner.enter(|| {
                let _ = self.inner.run();
                self.inner.poll_with(Some(Duration::ZERO));
                let _ = self.inner.run();
            });
        }

        pub fn park(&self, duration: Duration) {
            self.inner.enter(|| {
                self.inner.poll_with(Some(duration));
                let _ = self.inner.run();
            });
        }

        pub fn block_on<F>(&self, future: F) -> F::Output
        where
            F: Future,
        {
            self.inner.block_on(future)
        }
    }

    impl<T> JoinHandle<T> {
        #[must_use]
        pub fn is_finished(&self) -> bool {
            self.inner
                .as_ref()
                .map_or(true, compio::runtime::JoinHandle::is_finished)
        }
    }

    impl<T> Drop for JoinHandle<T> {
        fn drop(&mut self) {
            let _ = self.inner.take();
        }
    }

    impl<T> Future for JoinHandle<T> {
        type Output = Result<T, Box<dyn Any + Send + 'static>>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let Some(handle) = self.inner.as_mut() else {
                panic!("compio join handle polled after completion");
            };

            match Pin::new(handle).poll(cx) {
                Poll::Ready(result) => {
                    let _ = self.inner.take();
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl<T> std::fmt::Debug for JoinHandle<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("JoinHandle")
                .field("finished", &self.is_finished())
                .finish_non_exhaustive()
        }
    }

    impl std::fmt::Debug for Runtime {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Runtime").finish_non_exhaustive()
        }
    }

    pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
        compio::runtime::time::sleep(duration)
    }

    pub async fn yield_now() {
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
}

#[cfg(feature = "rt-tokio")]
mod imp {
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::{Context, Poll};
    use std::time::Duration;

    #[derive(Debug)]
    struct TaskCancelled;

    struct RuntimeState {
        runtime: tokio::runtime::Runtime,
        local_set: tokio::task::LocalSet,
    }

    #[derive(Clone)]
    pub struct Runtime {
        inner: Rc<RuntimeState>,
    }

    #[derive(Debug)]
    pub struct JoinHandle<T> {
        inner: Option<tokio::task::JoinHandle<T>>,
    }

    impl Runtime {
        pub fn new() -> io::Result<Self> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let local_set = tokio::task::LocalSet::new();

            Ok(Self {
                inner: Rc::new(RuntimeState { runtime, local_set }),
            })
        }

        #[must_use]
        pub fn ptr_eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.inner, &other.inner)
        }

        pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
        where
            F: Future + 'static,
            F::Output: 'static,
        {
            JoinHandle {
                inner: Some(self.inner.local_set.spawn_local(future)),
            }
        }

        pub fn spawn_blocking<F, T>(&self, work: F) -> JoinHandle<T>
        where
            F: FnOnce() -> T + Send + 'static,
            T: Send + 'static,
        {
            JoinHandle {
                inner: Some(self.inner.runtime.spawn_blocking(work)),
            }
        }

        pub fn drive_ready(&self) {
            self.block_on(async {
                tokio::task::yield_now().await;
            });
        }

        pub fn park(&self, duration: Duration) {
            self.block_on(async move {
                if duration.is_zero() {
                    tokio::task::yield_now().await;
                } else {
                    tokio::time::sleep(duration).await;
                }
            });
        }

        pub fn block_on<F>(&self, future: F) -> F::Output
        where
            F: Future,
        {
            self.inner.local_set.block_on(&self.inner.runtime, future)
        }
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
            let Some(handle) = self.inner.as_mut() else {
                panic!("tokio join handle polled after completion");
            };

            match Pin::new(handle).poll(cx) {
                Poll::Ready(Ok(result)) => {
                    let _ = self.inner.take();
                    Poll::Ready(Ok(result))
                }
                Poll::Ready(Err(err)) => {
                    let _ = self.inner.take();
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

    impl std::fmt::Debug for Runtime {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Runtime").finish_non_exhaustive()
        }
    }

    pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
        tokio::time::sleep(duration)
    }

    pub async fn yield_now() {
        tokio::task::yield_now().await;
    }
}

pub use imp::*;
