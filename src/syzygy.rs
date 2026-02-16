//! # Syzygy - Core/Shell Orchestration
//!
//! This module provides the `Syzygy`, a component that automates the interaction
//! between the `Core` and the `Shell`. It simplifies the process of building and
//! running a Syzygy application by managing the event loop.
//!
//! ## Key Components
//! - `Syzygy` - The main struct that drives the application by orchestrating `Core` and `Shell`.
//! - `SyzygyConfig` - Configuration for the syzygy's behavior.
//!
//! ## Example
//! ```rust,no_run
//! # use syzygy::prelude::*;
//! # use syzygy::executor::{InlineAsync, Task};
//! # #[derive(Debug, Clone)] enum TestEvent { Ping }
//! # #[derive(Debug, Clone)] enum TestEffect { DoPing }
//! # #[derive(Debug, Default)] struct Model;
//! # fn update(_event: TestEvent, _model: &mut Model) -> Command<TestEvent, TestEffect> { Command::none() }
//! # fn handle_effects(_effect: TestEffect, _resources: ()) -> Task<TestEvent, TestEffect> { Task::none() }
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // In a real application, you would build and run the system like this:
//! let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
//!     .model(Model::default())
//!     .event_handler(update)
//!     .effect_handler(handle_effects)
//!     .with_async_executor(InlineAsync::new())
//!     .build();
//!
//! // Run the application indefinitely
//! runner.run()?;
//! # Ok(())
//! # }
//! ```
use std::thread;
use std::time::{Duration, Instant};

use crate::command::Command;
use crate::core::Core;
use crate::error::ShellError;
use crate::shell::{Shell, ShellStats, ShellStatsSnapshot};

/// Configuration for the Syzygy
#[derive(Clone, Debug)]
pub struct SyzygyConfig {
    /// How often to yield control when no work is being done
    pub idle_sleep: Duration,
}

impl Default for SyzygyConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(16), // ~60 FPS
        }
    }
}

impl SyzygyConfig {
    /// Override the idle sleep interval with any type convertible to `Duration`.
    #[must_use]
    pub fn idle_sleep(mut self, duration: impl Into<Duration>) -> Self {
        self.idle_sleep = duration.into();
        self
    }
}

/// Syzygy automatically orchestrates Core/Shell interaction
///
/// This solves Grug's complaint about manual event loop orchestration.
/// Instead of users manually calling `poll_events` → process → execute → step,
/// Syzygy handles the proper sequencing automatically.
pub struct Syzygy<Event, Effect, Model, Resources = ()>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    core: Core<Event, Effect, Model>,
    shell: Shell<Event, Effect, Resources>,
    config: SyzygyConfig,
}

/// Terminology-friendly alias for [`Syzygy`], emphasizing its role as the runtime runner.
pub type Runner<Event, Effect, Model, Resources = ()> = Syzygy<Event, Effect, Model, Resources>;

impl<Event, Effect, Model, Resources> Syzygy<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    /// Create a new Syzygy with Core and Shell
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _model| Command::none())
    ///     .effect_handler(|_effect: Effect, _resources| Task::none())
    ///     .build()
    ///     .split();
    ///
    /// let syzygy = Syzygy::new(core, shell);
    /// ```
    pub fn new(core: Core<Event, Effect, Model>, shell: Shell<Event, Effect, Resources>) -> Self {
        Self {
            core,
            shell,
            config: SyzygyConfig::default(),
        }
    }

    /// Create a new Syzygy with custom configuration
    pub fn with_config(
        core: Core<Event, Effect, Model>,
        shell: Shell<Event, Effect, Resources>,
        config: SyzygyConfig,
    ) -> Self {
        Self {
            core,
            shell,
            config,
        }
    }

    /// Run the event loop continuously
    ///
    /// This will run until the shell is shut down or an error occurs.
    pub fn run(&mut self) -> Result<(), ShellError> {
        self.run_loop(Syzygy::step, |_| false)
    }

    /// Run until a condition is met
    ///
    /// Useful for testing or conditional execution.
    pub fn run_until<F>(&mut self, mut condition: F) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Resources>) -> bool,
    {
        self.run_loop(Syzygy::step, move |syzygy| {
            condition(&syzygy.core, &syzygy.shell)
        })
    }

    /// Drain the system for at most `max_steps` iterations.
    ///
    /// Returns the number of steps that performed work. Stops early if no work
    /// remains before hitting `max_steps`.
    pub fn drain_max(&mut self, max_steps: usize) -> Result<usize, ShellError> {
        let mut steps = 0;
        for _ in 0..max_steps {
            if !self.step()? {
                break;
            }
            steps += 1;
        }
        Ok(steps)
    }

    /// Drain until the predicate returns true or `timeout` elapses.
    pub fn drain_until<F>(&mut self, mut predicate: F, timeout: Duration) -> Result<(), ShellError>
    where
        F: FnMut(&Model) -> bool,
    {
        if predicate(self.core.model()) {
            return Ok(());
        }

        if timeout.is_zero() {
            return Err(ShellError::Timeout { duration: timeout });
        }

        let start = Instant::now();
        let deadline = start.checked_add(timeout);

        loop {
            if predicate(self.core.model()) {
                return Ok(());
            }

            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }
            }

            let did_work = self.step()?;

            if predicate(self.core.model()) {
                return Ok(());
            }

            if let Some(deadline) = deadline {
                let now = Instant::now();
                if now >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }

                if !did_work {
                    let remaining = deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO);
                    if remaining.is_zero() {
                        return Err(ShellError::Timeout { duration: timeout });
                    }

                    let had_work = self.wait_for_idle(remaining);

                    if predicate(self.core.model()) {
                        return Ok(());
                    }

                    if !had_work && Instant::now() >= deadline {
                        return Err(ShellError::Timeout { duration: timeout });
                    }
                }
            } else if !did_work {
                self.wait_for_work();
            }
        }
    }

    /// Drain the system until both Core and Shell report no outstanding work.
    pub fn drain_until_idle(&mut self, timeout: Duration) -> Result<(), ShellError> {
        if self.is_fully_idle() {
            return Ok(());
        }

        if timeout.is_zero() {
            return Err(ShellError::Timeout { duration: timeout });
        }

        let start = Instant::now();
        let deadline = start.checked_add(timeout);

        loop {
            let did_work = self.step()?;

            if self.is_fully_idle() {
                return Ok(());
            }

            if let Some(deadline) = deadline {
                let now = Instant::now();
                if now >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }

                if !did_work {
                    let remaining = deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO);
                    if remaining.is_zero() {
                        return Err(ShellError::Timeout { duration: timeout });
                    }

                    let had_work = self.wait_for_idle(remaining);

                    if self.is_fully_idle() {
                        return Ok(());
                    }

                    if !had_work && Instant::now() >= deadline {
                        return Err(ShellError::Timeout { duration: timeout });
                    }
                }
            } else if !did_work {
                self.wait_for_work();
            }
        }
    }

    /// Get an immutable reference to the model
    #[must_use]
    pub fn model(&self) -> &Model {
        self.core.model()
    }

    /// Get a mutable reference to the model
    ///
    /// This should be used carefully as it bypasses event processing.
    /// Prefer sending events for state changes.
    pub fn model_mut(&mut self) -> &mut Model {
        self.core.model_mut()
    }

    /// Get a reference to the Core
    pub fn core(&self) -> &Core<Event, Effect, Model> {
        &self.core
    }

    /// Get a mutable reference to the Core
    pub fn core_mut(&mut self) -> &mut Core<Event, Effect, Model> {
        &mut self.core
    }

    /// Get a reference to the Shell
    pub fn shell(&self) -> &Shell<Event, Effect, Resources> {
        &self.shell
    }

    /// Get a mutable reference to the Shell
    pub fn shell_mut(&mut self) -> &mut Shell<Event, Effect, Resources> {
        &mut self.shell
    }

    /// Get the syzygy configuration
    pub fn config(&self) -> &SyzygyConfig {
        &self.config
    }

    /// Update the syzygy configuration
    pub fn set_config(&mut self, config: SyzygyConfig) {
        self.config = config;
    }

    fn is_fully_idle(&self) -> bool {
        !self.core.has_pending_events() && self.shell.is_idle()
    }

    /// Request that the shell stop scheduling further work.
    pub fn shutdown(&mut self) {
        self.shell.shutdown();
        self.shell.wait_for_executors();

        #[cfg(feature = "tracing")]
        {
            let snapshot = self.shell.stats();
            if snapshot.dropped_events > 0 || snapshot.dropped_effect_steps > 0 {
                tracing::warn!(
                    stage = "syzygy_shutdown",
                    dropped_events = snapshot.dropped_events,
                    dropped_effect_steps = snapshot.dropped_effect_steps,
                    "Syzygy shutdown completed with dropped work"
                );
            } else {
                tracing::debug!(stage = "syzygy_shutdown", "Syzygy shutdown cleanly");
            }
        }
    }

    /// Wait until the system is completely idle or timeout expires
    ///
    /// System is considered idle when:
    /// - No pending events in core
    /// - No pending effects in shell
    /// - No in-flight async jobs
    ///
    /// Returns Err(ShellError::Timeout) if timeout expires before reaching idle.
    pub fn await_idle(&mut self, timeout: Duration) -> Result<(), ShellError> {
        if timeout.is_zero() {
            return Err(ShellError::Timeout { duration: timeout });
        }

        let start = Instant::now();
        let deadline = start.checked_add(timeout);

        loop {
            // Check if system is idle
            if self.core.pending_count() == 0
                && self.shell.pending_effects() == 0
                && self.shell.inflight_jobs() == 0
            {
                return Ok(());
            }

            // Check timeout
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }
            }

            // Process any available work
            let did_work = self.step()?;

            // If no work was done, wait for work to arrive
            if !did_work {
                let remaining = if let Some(deadline) = deadline {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(ShellError::Timeout { duration: timeout });
                    }
                    deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO)
                } else {
                    self.config.idle_sleep
                };

                // Wait for activity to reach zero or for new work to arrive
                if self.shell.inflight_jobs() > 0 {
                    if !self.shell.activity.wait_until_zero(remaining) {
                        // Timeout waiting for activity, but we might still have work
                        continue;
                    }
                } else {
                    let _ = self.wait_for_idle(remaining);
                }
            }
        }
    }

    /// Run until the system is completely idle (no timeout)
    ///
    /// This is a convenience method that calls `await_idle` with an infinite timeout.
    pub fn run_to_idle(&mut self) -> Result<(), ShellError> {
        self.await_idle(Duration::MAX)
    }

    /// Dispatch a command and then wait until the system is idle
    ///
    /// This is a convenience method that combines command dispatch with idle waiting.
    pub fn dispatch_and_await(
        &mut self,
        command: Command<Event, Effect>,
        timeout: Duration,
    ) -> Result<(), ShellError> {
        self.shell.dispatch_command(command)?;
        self.await_idle(timeout)
    }

    /// Wait for work to arrive, trying core first then shell
    ///
    /// Returns true if work arrived, false if we timed out
    #[must_use = "ignoring work detection defeats the purpose of waiting"]
    fn wait_for_idle(&mut self, duration: Duration) -> bool {
        let start = Instant::now();

        // Try core first - bail immediately if channel closed
        if self.core.wait_for_event(duration) {
            return true;
        }

        // If core didn't get work and shell is closed, bail
        if self.shell.is_closed() {
            return false;
        }

        // Calculate remaining time for shell
        let elapsed = start.elapsed();
        let remaining = match duration.checked_sub(elapsed) {
            Some(remaining) if !remaining.is_zero() => remaining,
            _ => return false, // No time left
        };

        // Try shell with remaining time and return its result
        self.shell.wait_for_effect(remaining)
    }

    /// Wait for work to arrive, blocking the current thread until either:
    /// - An event arrives in the core
    /// - An effect arrives in the shell
    /// - The idle sleep timeout expires
    /// - The shell is closed
    ///
    /// This method is called automatically by `run()` and `run_until()` when no work
    /// is available, but can also be called manually for fine-grained control.
    pub fn wait_for_work(&mut self) {
        // Early returns for cases where we shouldn't wait
        if self.shell.is_closed() {
            return;
        }

        if self.config.idle_sleep.is_zero() {
            thread::yield_now();
            return;
        }

        if self.core.has_pending_events() || self.shell.pending_effects() > 0 {
            return;
        }

        // Wait for work to arrive - we ignore the result since the main loop
        // will check for pending work on the next iteration anyway
        #[allow(unused_must_use)]
        {
            self.wait_for_idle(self.config.idle_sleep);
        }
    }

    fn run_loop<Step, Exit>(
        &mut self,
        mut step: Step,
        mut should_exit: Exit,
    ) -> Result<(), ShellError>
    where
        Step: FnMut(&mut Self) -> Result<bool, ShellError>,
        Exit: FnMut(&Self) -> bool,
    {
        loop {
            let did_work = step(self)?;

            if should_exit(self) || self.shell.is_closed() {
                break;
            }

            if !did_work {
                self.wait_for_work();
            }
        }

        Ok(())
    }

    /// Execute a single synchronous step of the event loop
    ///
    /// This processes events from Core and effects from Shell synchronously.
    /// Returns true if work was done, false if idle.
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let mut runner = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _model| Command::none())
    ///     .effect_handler(|_effect: Effect, _resources| Task::none())
    ///     .build();
    /// runner.core().try_send_event(Event::Test)?;
    ///
    /// // Process the event synchronously
    /// let did_work = runner.step()?;
    /// assert!(did_work);
    /// ```
    pub fn step(&mut self) -> Result<bool, ShellError> {
        step_core_shell(&mut self.core, &mut self.shell)
    }

    /// Snapshot Shell-level counters (dropped events/effects).
    #[must_use]
    pub fn shell_stats(&self) -> ShellStatsSnapshot {
        self.shell.stats()
    }

    /// Clone a shared stats handle for integration with metrics or tracing.
    #[must_use]
    pub fn shell_stats_handle(&self) -> ShellStats {
        self.shell.stats_handle()
    }

    /// Consume the runner and return ownership of the Core and Shell.
    pub fn split(self) -> (Core<Event, Effect, Model>, Shell<Event, Effect, Resources>) {
        (self.core, self.shell)
    }
}

/// Process pending events and effects once using the same logic as `Syzygy::step`.
pub fn step_core_shell<Event, Effect, Model, Resources>(
    core: &mut Core<Event, Effect, Model>,
    shell: &mut Shell<Event, Effect, Resources>,
) -> Result<bool, ShellError>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    let commands = core.process_events();
    let core_work = !commands.is_empty();
    for command in commands {
        shell.dispatch_command(command)?;
    }

    let shell_work = shell.drain()?;

    Ok(core_work || shell_work > 0)
}

impl<Event, Effect, Model, Resources>
    From<(Core<Event, Effect, Model>, Shell<Event, Effect, Resources>)>
    for Syzygy<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    fn from(parts: (Core<Event, Effect, Model>, Shell<Event, Effect, Resources>)) -> Self {
        Self::new(parts.0, parts.1)
    }
}

impl<Event, Effect, Model, Resources> std::fmt::Debug for Syzygy<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syzygy")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Syzygy<(), (), ()> {
    /// Create a new builder for Syzygy systems
    #[must_use]
    pub fn builder<NewEvent, NewEffect>(
    ) -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, (), ()>
    where
        NewEvent: Send + 'static,
        NewEffect: Send + 'static,
    {
        crate::builder::SyzygyBuilder::new()
    }
}
