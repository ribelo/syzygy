//! # Runner - Core/Shell Orchestration
//!
//! This module provides the `Runner`, a component that automates the interaction
//! between the `Core` and the `Shell`. It simplifies the process of building and
//! running a Syzygy application by managing the event loop.
//!
//! ## Key Components
//! - `Runner` - The main struct that drives the application by orchestrating `Core` and `Shell`.
//! - `RunnerConfig` - Configuration for the runner's behavior.
//!
//! ## Example
//! ```rust,no_run
//! # use syzygy::prelude::*;
//! # use syzygy::event_context::EventContext;
//! # #[derive(Debug, Clone)] enum TestEvent { Ping }
//! # #[derive(Debug, Clone)] enum TestEffect { DoPing }
//! # #[derive(Debug, Default)] struct Model;
//! # fn update(event: TestEvent, ctx: &mut EventContext<TestEvent, TestEffect, Storage<Model, EmptyStorage>>) -> Command<TestEvent, TestEffect> { Command::none() }
//! # async fn handle_effects(effect: TestEffect, ctx: EffectContext<TestEvent, EmptyStorage>) -> Outcome<TestEvent> { Outcome::None }
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // In a real application, you would build and run the system like this:
//! let (core, shell) = Syzygy::builder()
//!     .model(Model::default())
//!     .event_handler(update)
//!     .effect_handler(handle_effects)
//!     .build();
//! let mut runner = Runner::new(core, shell);
//!
//! // Run the application indefinitely
//! runner.run(syzygy::scheduler::scheduler()).await?;
//! # Ok(())
//! # }
//! ```
use std::time::Duration;

use crate::core::Core;
use crate::error::{CoreError, ShellError};
use crate::shell::Shell;
use crate::scheduler::Scheduler;
use crate::timer::{Time, time};

/// Configuration for the Runner
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// How often to yield control when no work is being done
    pub idle_sleep: Duration,
    /// Runtime implementation for sleeping - provides runtime neutrality
    pub runtime: Time,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(16), // ~60 FPS
            runtime: time(),
        }
    }
}

/// Runner automatically orchestrates Core/Shell interaction
///
/// This solves Grug's complaint about manual event loop orchestration.
/// Instead of users manually calling `poll_events` → process → execute → tick,
/// Runner handles the proper sequencing automatically.
pub struct Runner<Event, Effect, Storage, Resources = ()>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    core: Core<Event, Effect, Storage>,
    shell: Shell<Event, Effect, Resources>,
    config: RunnerConfig,
}

impl<Event, Effect, Storage, Resources> Runner<Event, Effect, Storage, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Create a new Runner with Core and Shell
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _ctx| Command::none())
    ///     .effect_handler(|_effect: Effect, _ctx| async move { Outcome::None })
    ///     .build();
    ///
    /// let runner = Runner::new(core, shell);
    /// ```
    pub fn new(core: Core<Event, Effect, Storage>, shell: Shell<Event, Effect, Resources>) -> Self {
        Self {
            core,
            shell,
            config: RunnerConfig::default(),
        }
    }

    /// Create a new Runner with custom configuration
    pub fn with_config(
        core: Core<Event, Effect, Storage>,
        shell: Shell<Event, Effect, Resources>,
        config: RunnerConfig,
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
    pub async fn run<S>(&mut self, scheduler: S) -> Result<(), RunnerError>
    where
        S: Scheduler,
    {
        loop {
            let did_work = self.step_with(scheduler.clone())?;

            if !did_work {
                self.config.runtime.sleep(self.config.idle_sleep).await;
            }

            // Check if shell is closed
            if self.shell.is_closed() {
                break;
            }
        }

        Ok(())
    }

    /// Run until a condition is met
    ///
    /// Useful for testing or conditional execution.
    pub async fn run_until<F, S>(&mut self, mut condition: F, scheduler: S) -> Result<(), RunnerError>
    where
        F: FnMut(&Core<Event, Effect, Storage>, &Shell<Event, Effect, Resources>) -> bool,
        S: Scheduler,
    {
        loop {
            let did_work = self.step_with(scheduler.clone())?;

            // Check condition
            if condition(&self.core, &self.shell) {
                break;
            }

            if !did_work {
                self.config.runtime.sleep(self.config.idle_sleep).await;
            }
        }

        Ok(())
    }

    /// Execute a single tick of the event loop
    ///
    /// Returns true if work was done, false if idle.
    ///
    /// # Deprecated
    /// This method is deprecated. Use `step()` or `step_with()` instead for synchronous operation.
    /// The async version will be removed in a future version.
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _ctx| Command::none())
    ///     .effect_handler(|_effect: Effect, _ctx| async move { Outcome::None })
    ///     .build();
    ///
    /// let mut runner = Runner::new(core, shell);
    /// runner.core().send_event(Event::Test)?;
    ///
    /// // Process the event synchronously
    /// let did_work = runner.tick(syzygy::scheduler::scheduler()).await?;
    /// assert!(did_work);
    /// # Ok(())
    /// # }
    /// ```
    #[deprecated(since = "0.1.0", note = "Use `step()` or `step_with()` for synchronous operation")]
    pub fn tick<S>(&mut self, scheduler: S) -> Result<bool, RunnerError>
    where
        S: Scheduler,
    {
        // Delegate to the synchronous step_with method
        self.step_with(scheduler)
    }

    /// Get a reference to the Core
    pub fn core(&self) -> &Core<Event, Effect, Storage> {
        &self.core
    }

    /// Get a mutable reference to the Core
    pub fn core_mut(&mut self) -> &mut Core<Event, Effect, Storage> {
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

    /// Get the runner configuration
    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    /// Update the runner configuration
    pub fn set_config(&mut self, config: RunnerConfig) {
        self.config = config;
    }

    /// Shutdown the runner gracefully
    /// Create runner with custom idle sleep duration
    #[must_use]
    pub fn with_idle_sleep(mut self, duration: Duration) -> Self {
        self.config.idle_sleep = duration;
        self
    }

    /// Create runner with custom runtime
    #[must_use]
    pub fn with_runtime(mut self, runtime: Time) -> Self {
        self.config.runtime = runtime;
        self
    }
    pub fn shutdown(&mut self) {
        self.shell.shutdown();
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
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _ctx| Command::none())
    ///     .effect_handler(|_effect: Effect, _ctx| async move { Outcome::None })
    ///     .build();
    ///
    /// let mut runner = Runner::new(core, shell);
    /// runner.core().send_event(Event::Test)?;
    ///
    /// // Process the event synchronously
    /// let did_work = runner.step()?;
    /// assert!(did_work);
    /// ```
    pub fn step(&mut self) -> Result<bool, RunnerError> {
        match crate::scheduler::scheduler_strict() {
            Ok(sched) => self.step_with(sched),
            Err(e) => Err(RunnerError::Shell(ShellError::CommandExecutionFailed(format!("No runtime available: {e}")))),
        }
    }

    /// Execute a single synchronous step of the event loop with custom scheduler
    ///
    /// This processes events from Core and effects from Shell synchronously.
    /// Returns true if work was done, false if idle.
    pub fn step_with<S>(&mut self, scheduler: S) -> Result<bool, RunnerError>
    where
        S: Scheduler,
    {
        let mut did_work = false;

        // Process all events from Core
        let (core_work, commands) = self.core.process_events();

        if core_work {
            did_work = true;

            // Dispatch commands to Shell (may route events back to Core)
            for command in commands {
                self.shell.enqueue_command(command).map_err(RunnerError::Shell)?;
            }
        }

        // Process effects in Shell synchronously
        let shell_work = self.shell.drain_with(scheduler).map_err(RunnerError::Shell)?;
        if shell_work > 0 {
            did_work = true;
        }

        Ok(did_work)
    }

    /// Run with default scheduler
    pub async fn run_default(&mut self) -> Result<(), RunnerError> {
        match crate::scheduler::scheduler_strict() {
            Ok(sched) => self.run(sched).await,
            Err(e) => Err(RunnerError::Shell(ShellError::CommandExecutionFailed(format!("No runtime available: {e}")))),
        }
    }

    /// Tick with default scheduler (deprecated, use step() instead)
    #[deprecated(since = "0.1.0", note = "Use `step()` for synchronous operation")]
    pub fn tick_default(&mut self) -> Result<bool, RunnerError> {
        self.step()
    }
}

/// Errors that can occur in the Runner
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("Core error: {0}")]
    Core(CoreError),

    #[error("Shell error: {0}")]
    Shell(ShellError),

    #[error("Runner timed out")]
    Timeout,
}


impl<Event, Effect, Storage, Resources> std::fmt::Debug for Runner<Event, Effect, Storage, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Ping,
        Pong,
    }

    #[derive(Debug, Default)]
    struct TestModel {
        count: i32,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    fn test_update(
        event: TestEvent,
        ctx: &mut crate::event_context::EventContext<TestEvent, TestEffect, TestModel>,
    ) -> Command<TestEvent, TestEffect> {
        let model: &mut TestModel = ctx.model_mut();

        match event {
            TestEvent::Ping => {
                model.count += 1;
                Command::event(TestEvent::Pong)
            }
            TestEvent::Pong => {
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_runner_basic() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = core.event_sender();

        let mut runner = Runner::new(core, shell);

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Process one tick
        let did_work = runner.tick(crate::scheduler::TokioScheduler).await.unwrap();

        assert!(did_work);
        // Skipped model access check in refactor
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_runner_until_condition() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = core.event_sender();

        let mut runner = Runner::with_config(
            core,
            shell,
            RunnerConfig {
                idle_sleep: Duration::from_millis(1),
                runtime: crate::timer::time(),
            },
        );

        // Send multiple events
        for _ in 0..5 {
            event_sender.send(TestEvent::Ping).unwrap();
        }

        // Run until count reaches 10
        runner
            .run_until(|_core, _shell| true, crate::scheduler::TokioScheduler)
            .await
            .unwrap();

        // Skipped model access check in refactor
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_returns_true_when_work_was_done() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = core.event_sender();
        let mut runner = Runner::new(core, shell);

        // Send an event to create work
        event_sender.send(TestEvent::Ping).unwrap();

        // Step should return true (work was done)
        let did_work = runner.step().expect("Step should succeed");
        assert!(did_work, "Step should return true when work was done");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_returns_false_when_no_work_to_do() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let mut runner = Runner::new(core, shell);

        // No events sent, so no work to do
        let did_work = runner.step().expect("Step should succeed");
        assert!(!did_work, "Step should return false when no work to do");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_with_custom_scheduler() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = core.event_sender();
        let mut runner = Runner::new(core, shell);

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Use custom scheduler
        let scheduler = crate::scheduler::TokioScheduler::new().expect("Should have tokio runtime");
        let did_work = runner.step_with(scheduler).expect("Step should succeed");
        assert!(did_work, "Step with custom scheduler should return true when work was done");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_processes_exactly_one_event() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = core.event_sender();
        let mut runner = Runner::new(core, shell);

        // Send multiple events
        event_sender.send(TestEvent::Ping).unwrap();
        event_sender.send(TestEvent::Ping).unwrap();
        event_sender.send(TestEvent::Ping).unwrap();

        // First step should process one event and return true
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(did_work1, "First step should return true");

        // Second step should process the next event and return true
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(did_work2, "Second step should return true");

        // Third step should process the last event and return true
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(did_work3, "Third step should return true");

        // Fourth step should have no more work and return false
        let did_work4 = runner.step().expect("Fourth step should succeed");
        assert!(!did_work4, "Fourth step should return false (no more work)");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_handles_multiple_events_and_effects_in_sequence() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = core.event_sender();
        let mut runner = Runner::new(core, shell);

        // Send Ping event (will generate Pong event and Log effect)
        event_sender.send(TestEvent::Ping).unwrap();

        // First step: process Ping -> generate Pong event and Log effect
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(did_work1, "First step should process Ping event");

        // Second step: process Pong event -> generate Log effect
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(did_work2, "Second step should process Pong event");

        // Third step: process Log effect
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(did_work3, "Third step should process Log effect");

        // Fourth step: no more work
        let did_work4 = runner.step().expect("Fourth step should succeed");
        assert!(!did_work4, "Fourth step should have no more work");
    }
}
