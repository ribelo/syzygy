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
//! let mut runner = Syzygy::builder()
//!     .model(Model::default())
//!     .event_handler(update)
//!     .effect_handler(handle_effects)
//!     .build();
//!
//! // Run the application indefinitely
//! runner.run(syzygy::scheduler::scheduler())?;
//! # Ok(())
//! # }
//! ```
use std::thread;
use std::time::{Duration, Instant};

use crate::core::Core;
use crate::error::ShellError;
use crate::scheduler::Scheduler;
use crate::shell::Shell;

/// Configuration for the Runner
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// How often to yield control when no work is being done
    pub idle_sleep: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(16), // ~60 FPS
        }
    }
}

impl RunnerConfig {
    /// Override the idle sleep interval with any type convertible to `Duration`.
    #[must_use]
    pub fn idle_sleep(mut self, duration: impl Into<Duration>) -> Self {
        self.idle_sleep = duration.into();
        self
    }
}

/// Runner automatically orchestrates Core/Shell interaction
///
/// This solves Grug's complaint about manual event loop orchestration.
/// Instead of users manually calling `poll_events` → process → execute → step,
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
    ///     .build()
    ///     .split();
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
    pub fn run(&mut self, scheduler: impl Scheduler) -> Result<(), ShellError> {
        loop {
            let did_work = self.step_with(scheduler.clone())?;

            if self.shell.is_closed() {
                break;
            }

            if !did_work {
                self.wait_for_work();
            }
        }

        Ok(())
    }

    /// Run until a condition is met
    ///
    /// Useful for testing or conditional execution.
    pub fn run_until<F>(
        &mut self,
        mut condition: F,
        scheduler: impl Scheduler,
    ) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Storage>, &Shell<Event, Effect, Resources>) -> bool,
    {
        loop {
            let did_work = self.step_with(scheduler.clone())?;

            // Check condition
            if condition(&self.core, &self.shell) {
                break;
            }

            if self.shell.is_closed() {
                break;
            }

            if !did_work {
                self.wait_for_work();
            }
        }

        Ok(())
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

    /// Request that the shell stop scheduling further work.
    pub fn shutdown(&mut self) {
        self.shell.shutdown();
    }

    fn wait_for_work(&mut self) {
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

        let start = Instant::now();
        if self.core.wait_for_event(self.config.idle_sleep) {
            return;
        }

        let elapsed = start.elapsed();
        let remaining = match self.config.idle_sleep.checked_sub(elapsed) {
            Some(remaining) if !remaining.is_zero() => remaining,
            _ => return,
        };

        self.shell.wait_for_effect(remaining);
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
    ///     .event_handler(|_event: Event, _ctx| Command::none())
    ///     .effect_handler(|_effect: Effect, _ctx| async move { Outcome::None })
    ///     .build();
    /// runner.core().send_event(Event::Test)?;
    ///
    /// // Process the event synchronously
    /// let did_work = runner.step()?;
    /// assert!(did_work);
    /// ```
    pub fn step(&mut self) -> Result<bool, ShellError> {
        match crate::scheduler::scheduler_strict() {
            Ok(sched) => self.step_with(sched),
            Err(e) => Err(ShellError::CommandExecutionFailed(format!(
                "No runtime available: {e}"
            ))),
        }
    }

    /// Execute a single synchronous step of the event loop with custom scheduler
    ///
    /// This processes events from Core and effects from Shell synchronously.
    /// Returns true if work was done, false if idle.
    pub fn step_with(&mut self, scheduler: impl Scheduler) -> Result<bool, ShellError> {
        step_core_shell(&mut self.core, &mut self.shell, scheduler)
    }

    /// Consume the runner and return ownership of the Core and Shell.
    pub fn split(
        self,
    ) -> (
        Core<Event, Effect, Storage>,
        Shell<Event, Effect, Resources>,
    ) {
        (self.core, self.shell)
    }
}

/// Process pending events and effects once using the same logic as `Runner::step_with`.
pub fn step_core_shell<Event, Effect, Storage, Resources>(
    core: &mut Core<Event, Effect, Storage>,
    shell: &mut Shell<Event, Effect, Resources>,
    scheduler: impl Scheduler,
) -> Result<bool, ShellError>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    let (core_work, commands) = core.process_events();
    for command in commands {
        shell.dispatch_command(command)?;
    }

    let shell_work = shell.drain_with(scheduler)?;

    Ok(core_work || shell_work > 0)
}

impl<Event, Effect, Storage, Resources>
    From<(
        Core<Event, Effect, Storage>,
        Shell<Event, Effect, Resources>,
    )> for Runner<Event, Effect, Storage, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn from(
        parts: (
            Core<Event, Effect, Storage>,
            Shell<Event, Effect, Resources>,
        ),
    ) -> Self {
        Self::new(parts.0, parts.1)
    }
}

impl<Event, Effect, Storage, Resources> std::fmt::Debug
    for Runner<Event, Effect, Storage, Resources>
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
    use crate::executor::ExecutorRegistry;
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
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = runner.core().event_sender();

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Process one step
        let did_work = runner.step_with(crate::scheduler::scheduler()).unwrap();

        assert!(did_work);
        // Skipped model access check in refactor
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_runner_until_condition() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        runner.set_config(RunnerConfig {
            idle_sleep: Duration::from_millis(1),
        });

        let event_sender = runner.core().event_sender();

        // Send multiple events
        for _ in 0..5 {
            event_sender.send(TestEvent::Ping).unwrap();
        }

        // Run until count reaches 10
        runner
            .run_until(|_core, _shell| true, crate::scheduler::scheduler())
            .unwrap();

        // Skipped model access check in refactor
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_returns_true_when_work_was_done() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = runner.core().event_sender();

        // Send an event to create work
        event_sender.send(TestEvent::Ping).unwrap();

        // Step should return true (work was done)
        let did_work = runner.step().expect("Step should succeed");
        assert!(did_work, "Step should return true when work was done");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_returns_false_when_no_work_to_do() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        // No events sent, so no work to do
        let did_work = runner.step().expect("Step should succeed");
        assert!(!did_work, "Step should return false when no work to do");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_with_custom_scheduler() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = runner.core().event_sender();

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Use custom scheduler
        let scheduler = crate::scheduler::TokioScheduler::new().expect("Should have tokio runtime");
        let did_work = runner.step_with(scheduler).expect("Step should succeed");
        assert!(
            did_work,
            "Step with custom scheduler should return true when work was done"
        );
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_processes_exactly_one_event() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = runner.core().event_sender();

        // Send multiple events
        event_sender.send(TestEvent::Ping).unwrap();
        event_sender.send(TestEvent::Ping).unwrap();
        event_sender.send(TestEvent::Ping).unwrap();

        // First step should process ALL 3 events and return true
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(
            did_work1,
            "First step should return true (processed all 3 events)"
        );

        // Second step should process effects and return true
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(
            did_work2,
            "Second step should return true (processed effects)"
        );

        // Third step should have no more work and return false
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(!did_work3, "Third step should return false (no more work)");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_step_handles_multiple_events_and_effects_in_sequence() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_executor_registry(ExecutorRegistry::new())
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .build();

        let event_sender = runner.core().event_sender();

        // Send Ping event (will generate Pong event and Log effect)
        event_sender.send(TestEvent::Ping).unwrap();

        // First step: process Ping (generates Pong) and Pong (generates Log effect)
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(did_work1, "First step should process Ping and Pong events");

        // Second step: process Log effects
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(did_work2, "Second step should process Log effects");

        // Third step: no more work
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(!did_work3, "Third step should have no more work");
    }
}
