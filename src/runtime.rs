//! Owned async runtime abstractions used by shell execution.

#[cfg(all(feature = "rt-compio", feature = "rt-tokio"))]
compile_error!("features `rt-compio` and `rt-tokio` are mutually exclusive");

#[cfg(not(any(feature = "rt-compio", feature = "rt-tokio")))]
compile_error!("a runtime feature must be enabled: `rt-compio` or `rt-tokio`");

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

type BackendSleep = Pin<Box<dyn Future<Output = ()> + 'static>>;

thread_local! {
    static CURRENT_CLOCK: RefCell<Option<Clock>> = const { RefCell::new(None) };
}

#[derive(Clone)]
enum Clock {
    Real,
    Manual(Arc<Mutex<ManualClockState>>),
}

impl Clock {
    #[must_use]
    fn real() -> Self {
        Self::Real
    }

    #[must_use]
    fn manual() -> (Self, ManualClock) {
        let state = Arc::new(Mutex::new(ManualClockState::new()));
        let clock = ManualClock {
            state: Arc::clone(&state),
        };
        (Self::Manual(state), clock)
    }

    #[must_use]
    fn is_manual(&self) -> bool {
        matches!(self, Self::Manual(_))
    }

    #[must_use]
    fn bind_sleep(&self, duration: Duration) -> SleepBinding {
        if duration.is_zero() {
            return SleepBinding::Ready;
        }

        match self {
            Self::Real => SleepBinding::Real(imp::backend_sleep(duration)),
            Self::Manual(state) => SleepBinding::Manual(ManualSleep::new(
                ManualClock {
                    state: Arc::clone(state),
                },
                duration,
            )),
        }
    }
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Real => f.write_str("Clock::Real"),
            Self::Manual(_) => f.write_str("Clock::Manual"),
        }
    }
}

enum SleepBinding {
    Ready,
    Real(BackendSleep),
    Manual(ManualSleep),
}

struct ManualClockState {
    now: Duration,
    next_sleeper_id: u64,
    sleepers: Vec<SleeperRegistration>,
}

impl ManualClockState {
    #[must_use]
    fn new() -> Self {
        Self {
            now: Duration::ZERO,
            next_sleeper_id: 0,
            sleepers: Vec::new(),
        }
    }
}

struct SleeperRegistration {
    id: u64,
    deadline: Duration,
    waker: Waker,
}

#[derive(Clone)]
pub struct ManualClock {
    state: Arc<Mutex<ManualClockState>>,
}

impl ManualClock {
    pub fn advance(&self, duration: Duration) {
        let mut ready_wakers = Vec::new();
        {
            let mut state = lock_manual_clock_state(&self.state);
            state.now = state
                .now
                .checked_add(duration)
                .expect("manual clock overflow while advancing time");

            let now = state.now;
            let mut index = 0usize;
            while index < state.sleepers.len() {
                if state.sleepers[index].deadline <= now {
                    ready_wakers.push(state.sleepers.swap_remove(index).waker);
                } else {
                    index += 1;
                }
            }
        }

        for waker in ready_wakers {
            waker.wake();
        }
    }

    #[must_use]
    pub fn now(&self) -> Duration {
        lock_manual_clock_state(&self.state).now
    }

    fn register_or_refresh(
        &self,
        registration_id: Option<u64>,
        deadline: Duration,
        waker: &Waker,
    ) -> Option<u64> {
        let mut state = lock_manual_clock_state(&self.state);
        if deadline <= state.now {
            return None;
        }

        if let Some(id) = registration_id {
            for sleeper in &mut state.sleepers {
                if sleeper.id == id {
                    sleeper.deadline = deadline;
                    if !sleeper.waker.will_wake(waker) {
                        sleeper.waker.clone_from(waker);
                    }
                    return Some(id);
                }
            }
        }

        let id = state.next_sleeper_id;
        state.next_sleeper_id = state
            .next_sleeper_id
            .checked_add(1)
            .expect("manual clock sleeper id overflow");
        state.sleepers.push(SleeperRegistration {
            id,
            deadline,
            waker: waker.clone(),
        });
        Some(id)
    }

    fn cancel(&self, registration_id: Option<u64>) {
        let Some(registration_id) = registration_id else {
            return;
        };

        let mut state = lock_manual_clock_state(&self.state);
        let mut index = 0usize;
        while index < state.sleepers.len() {
            if state.sleepers[index].id == registration_id {
                let _ = state.sleepers.swap_remove(index);
                return;
            }
            index += 1;
        }
    }
}

impl std::fmt::Debug for ManualClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock_manual_clock_state(&self.state);
        f.debug_struct("ManualClock")
            .field("now", &state.now)
            .field("sleepers", &state.sleepers.len())
            .finish()
    }
}

fn lock_manual_clock_state(
    state: &Arc<Mutex<ManualClockState>>,
) -> std::sync::MutexGuard<'_, ManualClockState> {
    state
        .lock()
        .unwrap_or_else(|_| panic!("manual clock state mutex poisoned"))
}

struct ManualSleep {
    clock: ManualClock,
    deadline: Duration,
    registration_id: Option<u64>,
}

impl ManualSleep {
    #[must_use]
    fn new(clock: ManualClock, duration: Duration) -> Self {
        let deadline = clock
            .now()
            .checked_add(duration)
            .expect("manual clock overflow while scheduling sleep");
        Self {
            clock,
            deadline,
            registration_id: None,
        }
    }
}

impl Future for ManualSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.clock.now() >= self.deadline {
            let registration_id = self.registration_id.take();
            self.clock.cancel(registration_id);
            return Poll::Ready(());
        }

        let Some(registration_id) =
            self.clock
                .register_or_refresh(self.registration_id, self.deadline, cx.waker())
        else {
            let registration_id = self.registration_id.take();
            self.clock.cancel(registration_id);
            return Poll::Ready(());
        };
        self.registration_id = Some(registration_id);
        Poll::Pending
    }
}

impl Drop for ManualSleep {
    fn drop(&mut self) {
        self.clock.cancel(self.registration_id.take());
    }
}

enum SleepState {
    Unbound,
    Real(BackendSleep),
    Manual(ManualSleep),
    Done,
}

pub struct Sleep {
    duration: Duration,
    state: SleepState,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match &mut self.state {
                SleepState::Unbound => {
                    let clock = current_clock()
                        .unwrap_or_else(|| panic!("syzygy::runtime::sleep polled outside Runtime"));
                    self.state = match clock.bind_sleep(self.duration) {
                        SleepBinding::Ready => SleepState::Done,
                        SleepBinding::Real(future) => SleepState::Real(future),
                        SleepBinding::Manual(future) => SleepState::Manual(future),
                    };
                }
                SleepState::Real(future) => match future.as_mut().poll(cx) {
                    Poll::Ready(()) => {
                        self.state = SleepState::Done;
                        return Poll::Ready(());
                    }
                    Poll::Pending => return Poll::Pending,
                },
                SleepState::Manual(future) => match Pin::new(future).poll(cx) {
                    Poll::Ready(()) => {
                        self.state = SleepState::Done;
                        return Poll::Ready(());
                    }
                    Poll::Pending => return Poll::Pending,
                },
                SleepState::Done => return Poll::Ready(()),
            }
        }
    }
}

#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        duration,
        state: SleepState::Unbound,
    }
}

fn current_clock() -> Option<Clock> {
    CURRENT_CLOCK.with(|current| current.borrow().clone())
}

fn with_current_clock<T>(clock: &Clock, f: impl FnOnce() -> T) -> T {
    CURRENT_CLOCK.with(|current| {
        let previous = current.replace(Some(clock.clone()));
        let result = f();
        let _ = current.replace(previous);
        result
    })
}

#[cfg(feature = "rt-compio")]
mod imp {
    use std::any::Any;
    use std::future::{poll_fn, Future};
    use std::io;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use super::{with_current_clock, BackendSleep, Clock, ManualClock};

    #[derive(Clone)]
    pub struct Runtime {
        inner: Rc<compio::runtime::Runtime>,
        clock: Clock,
    }

    pub struct JoinHandle<T> {
        inner: Option<compio::runtime::JoinHandle<T>>,
    }

    impl Runtime {
        pub fn new() -> io::Result<Self> {
            Self::with_clock(Clock::real())
        }

        pub fn manual() -> io::Result<(Self, ManualClock)> {
            let (clock, manual_clock) = Clock::manual();
            let runtime = Self::with_clock(clock)?;
            Ok((runtime, manual_clock))
        }

        fn with_clock(clock: Clock) -> io::Result<Self> {
            Ok(Self {
                inner: Rc::new(compio::runtime::Runtime::new()?),
                clock,
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
            with_current_clock(&self.clock, || {
                self.inner.enter(|| {
                    let _ = self.inner.run();
                    self.inner.poll_with(Some(Duration::ZERO));
                    let _ = self.inner.run();
                });
            });
        }

        pub fn park(&self, duration: Duration) {
            if self.clock.is_manual() {
                self.drive_ready();
                return;
            }

            with_current_clock(&self.clock, || {
                self.inner.enter(|| {
                    self.inner.poll_with(Some(duration));
                    let _ = self.inner.run();
                });
            });
        }

        pub fn block_on<F>(&self, future: F) -> F::Output
        where
            F: Future,
        {
            with_current_clock(&self.clock, || self.inner.block_on(future))
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
            f.debug_struct("Runtime")
                .field("clock", &self.clock)
                .finish_non_exhaustive()
        }
    }

    pub(crate) fn backend_sleep(duration: Duration) -> BackendSleep {
        Box::pin(compio::runtime::time::sleep(duration))
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

    use super::{with_current_clock, BackendSleep, Clock, ManualClock};

    #[derive(Debug)]
    struct TaskCancelled;

    struct RuntimeState {
        runtime: tokio::runtime::Runtime,
        local_set: tokio::task::LocalSet,
    }

    #[derive(Clone)]
    pub struct Runtime {
        inner: Rc<RuntimeState>,
        clock: Clock,
    }

    #[derive(Debug)]
    pub struct JoinHandle<T> {
        inner: Option<tokio::task::JoinHandle<T>>,
    }

    impl Runtime {
        pub fn new() -> io::Result<Self> {
            Self::with_clock(Clock::real())
        }

        pub fn manual() -> io::Result<(Self, ManualClock)> {
            let (clock, manual_clock) = Clock::manual();
            let runtime = Self::with_clock(clock)?;
            Ok((runtime, manual_clock))
        }

        fn with_clock(clock: Clock) -> io::Result<Self> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let local_set = tokio::task::LocalSet::new();

            Ok(Self {
                inner: Rc::new(RuntimeState { runtime, local_set }),
                clock,
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
            if self.clock.is_manual() {
                self.drive_ready();
                return;
            }

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
            with_current_clock(&self.clock, || {
                self.inner.local_set.block_on(&self.inner.runtime, future)
            })
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
            f.debug_struct("Runtime")
                .field("clock", &self.clock)
                .finish_non_exhaustive()
        }
    }

    pub(crate) fn backend_sleep(duration: Duration) -> BackendSleep {
        Box::pin(tokio::time::sleep(duration))
    }

    pub async fn yield_now() {
        tokio::task::yield_now().await;
    }
}

pub use imp::{yield_now, JoinHandle, Runtime};

#[cfg(test)]
mod tests {
    use futures::task::noop_waker_ref;

    use super::*;

    #[test]
    fn manual_clock_registration_returns_ready_when_time_already_advanced() {
        let (_clock, manual_clock) = Clock::manual();
        let waker = noop_waker_ref();

        manual_clock.advance(Duration::from_secs(2));
        let registration = manual_clock.register_or_refresh(None, Duration::from_secs(1), waker);

        assert!(registration.is_none());
    }
}
