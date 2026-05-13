//! Deterministic shell-level test harness backed by a real `Syzygy` runner.

use std::time::Duration;
use std::time::Instant;

use crate::error::{CoreError, ShellError};
use crate::runtime::ManualClock;
use crate::syzygy::Syzygy;

/// Default bound for [`RunnerTester::drain`].
///
/// Keeps tests from silently spinning forever when a command chain never settles.
const DEFAULT_MAX_DRAIN_STEPS: usize = 10_000;

/// Deterministic harness for shell-owned runtime behavior.
///
/// `RunnerTester` owns a real [`Syzygy`] instance configured with a manual
/// runtime clock. Use it when a test must exercise subscriptions, processes,
/// runtime-owned tasks, or shell errors that [`TestStore`](crate::test_store::TestStore)
/// cannot model honestly.
pub struct RunnerTester<E, X, M, Resources = ()>
where
    E: 'static,
    X: 'static,
    M: 'static,
    Resources: 'static,
{
    runner: Syzygy<E, X, M, Resources>,
    clock: ManualClock,
    max_drain_steps: usize,
}

impl<E, X, M, Resources> RunnerTester<E, X, M, Resources>
where
    E: 'static,
    X: 'static,
    M: 'static,
    Resources: 'static,
{
    pub(crate) fn new(runner: Syzygy<E, X, M, Resources>, clock: ManualClock) -> Self {
        Self {
            runner,
            clock,
            max_drain_steps: DEFAULT_MAX_DRAIN_STEPS,
        }
    }

    #[must_use]
    pub fn with_max_drain_steps(mut self, max_drain_steps: usize) -> Self {
        assert!(
            max_drain_steps > 0,
            "RunnerTester max_drain_steps must be greater than zero"
        );
        self.max_drain_steps = max_drain_steps;
        self
    }

    pub fn try_send(&mut self, event: E) -> Result<&mut Self, CoreError> {
        self.runner.core().try_send(event)?;
        Ok(self)
    }

    pub fn send(&mut self, event: E) -> &mut Self {
        self.try_send(event)
            .unwrap_or_else(|error| panic!("RunnerTester::send failed: {error}"))
    }

    pub fn step(&mut self) -> Result<bool, ShellError> {
        self.runner.step()
    }

    pub fn drain(&mut self) -> Result<usize, ShellError> {
        let mut executed_steps = 0usize;
        loop {
            assert!(
                executed_steps < self.max_drain_steps,
                "RunnerTester::drain exceeded max_drain_steps={} (likely infinite loop)",
                self.max_drain_steps
            );

            let did_work = self.runner.step()?;
            if !did_work {
                return Ok(executed_steps);
            }

            executed_steps += 1;
        }
    }

    pub fn wait_for<F>(&mut self, timeout: Duration, mut condition: F) -> Result<bool, ShellError>
    where
        F: FnMut(&Syzygy<E, X, M, Resources>) -> bool,
    {
        if condition(&self.runner) {
            return Ok(true);
        }

        let deadline = Instant::now().checked_add(timeout);
        let mut iterations = 0usize;
        loop {
            if deadline.is_none() {
                assert!(
                    iterations < self.max_drain_steps,
                    "RunnerTester::wait_for exceeded max_drain_steps={} before meeting its condition",
                    self.max_drain_steps
                );
            }

            let did_work = self.runner.step()?;
            iterations += 1;
            if condition(&self.runner) {
                return Ok(true);
            }

            if did_work {
                continue;
            }

            let Some(deadline) = deadline else {
                self.runner
                    .shell()
                    .park_runtime(self.runner.config().idle_sleep);
                continue;
            };

            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }

            let remaining = deadline.saturating_duration_since(now);
            let park_duration = self.runner.config().idle_sleep.min(remaining);
            self.runner.shell().park_runtime(park_duration);
        }
    }

    pub fn advance_time(&mut self, duration: Duration) -> &mut Self {
        self.clock.advance(duration);
        self
    }

    #[must_use]
    pub fn now(&self) -> Duration {
        self.clock.now()
    }

    #[must_use]
    pub fn state(&self) -> &M {
        self.runner.model()
    }

    pub fn state_mut(&mut self) -> &mut M {
        self.runner.model_mut()
    }

    pub fn assert_state(&self, expected: &M)
    where
        M: PartialEq + std::fmt::Debug,
    {
        assert_eq!(self.state(), expected, "unexpected state");
    }

    pub fn assert_state_changed(&self, check: impl FnOnce(&M)) {
        check(self.state());
    }

    #[must_use]
    pub fn runner(&self) -> &Syzygy<E, X, M, Resources> {
        &self.runner
    }

    pub fn runner_mut(&mut self) -> &mut Syzygy<E, X, M, Resources> {
        &mut self.runner
    }

    pub fn into_runner(self) -> Syzygy<E, X, M, Resources> {
        self.runner
    }
}
