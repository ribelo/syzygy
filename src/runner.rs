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
//! # use syzygy::storage::{EmptyStorage, Storage};
//! # #[derive(Debug, Clone)] enum TestEvent { Ping }
//! # #[derive(Debug, Clone)] enum TestEffect { DoPing }
//! # #[derive(Debug, Default)] struct Model;
//! # fn update(event: TestEvent, ctx: &mut EventContext<TestEvent, TestEffect, Storage<Model, EmptyStorage>>) -> Command<TestEvent, TestEffect> { Command::none() }
//! # async fn handle_effects(effect: TestEffect, ctx: EffectContext<TestEvent, EmptyStorage>) -> EffectOutput<TestEvent> { EffectOutput::None }
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
//! runner.run(syzygy::spawn::spawner()).await?;
//! # Ok(())
//! # }
//! ```
use std::time::Duration;

use crate::core::Core;
use crate::error::{CoreError, ShellError};
use crate::shell::Shell;
use crate::spawn::Spawn;
use crate::timer::{Time, time};

/// Configuration for the Runner
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// How often to yield control when no work is being done
    pub idle_sleep: Duration,
    /// Maximum time to run before yielding (for run_until scenarios)
    pub max_run_duration: Option<Duration>,
    /// Whether to print debug info about work being done
    pub debug_logging: bool,
    /// Runtime implementation for sleeping - provides runtime neutrality
    pub runtime: Time,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(16), // ~60 FPS
            max_run_duration: None,
            debug_logging: false,
            runtime: time(),
        }
    }
}

/// Runner automatically orchestrates Core/Shell interaction
///
/// This solves Grug's complaint about manual event loop orchestration.
/// Instead of users manually calling poll_events → process → execute → tick,
/// Runner handles the proper sequencing automatically.
pub struct Runner<Event, Effect, Storage, Resources = (), Executors = (), H = ()>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
    H: crate::effect_handler::EffectHandler<Event, Effect, Resources, Executors> + Clone + 'static,
{
    core: Core<Event, Effect, Storage>,
    shell: Shell<Event, Effect, Resources, Executors, H>,
    config: RunnerConfig,
}

impl<Event, Effect, Storage, Resources, Executors, H>
    Runner<Event, Effect, Storage, Resources, Executors, H>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
    Executors: Clone + Send + Sync + 'static,
    H: crate::effect_handler::EffectHandler<Event, Effect, Resources, Executors> + Clone + 'static,
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
    ///     .effect_handler(|_effect: Effect, _ctx| async move { EffectOutput::None })
    ///     .build();
    ///
    /// let runner = Runner::new(core, shell);
    /// ```
    pub fn new(
        core: Core<Event, Effect, Storage>,
        shell: Shell<Event, Effect, Resources, Executors, H>,
    ) -> Self {
        Self {
            core,
            shell,
            config: RunnerConfig::default(),
        }
    }

    /// Create a new Runner with custom configuration
    pub fn with_config(
        core: Core<Event, Effect, Storage>,
        shell: Shell<Event, Effect, Resources, Executors, H>,
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
    pub async fn run<S>(&mut self, spawner: S) -> Result<(), RunnerError>
    where
        S: Spawn,
    {
        loop {
            let did_work = self.tick(spawner.clone()).await?;

            if !did_work {
                self.config.runtime.sleep(self.config.idle_sleep).await;
            }

            // Check if shell is closed
            if self.shell.task_stats().is_closed {
                break;
            }
        }

        Ok(())
    }

    /// Run until a condition is met
    ///
    /// Useful for testing or conditional execution.
    pub async fn run_until<F, S>(&mut self, mut condition: F, spawner: S) -> Result<(), RunnerError>
    where
        F: FnMut(&Core<Event, Effect, Storage>, &Shell<Event, Effect, Resources, Executors, H>) -> bool,
        S: Spawn,
    {
        let start_time = std::time::Instant::now();

        loop {
            let did_work = self.tick(spawner.clone()).await?;

            // Check condition
            if condition(&self.core, &self.shell) {
                if self.config.debug_logging {
                    println!("Runner: Condition met, stopping");
                }
                break;
            }

            // Check timeout
            if let Some(max_duration) = self.config.max_run_duration
                && start_time.elapsed() > max_duration
            {
                return Err(RunnerError::Timeout);
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
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _ctx| Command::none())
    ///     .effect_handler(|_effect: Effect, _ctx| async move { EffectOutput::None })
    ///     .build();
    ///
    /// let mut runner = Runner::new(core, shell);
    /// runner.core().send_event(Event::Test)?;
    ///
    /// // Process the event
    /// let did_work = runner.tick(syzygy::spawn::spawner()).await?;
    /// assert!(did_work);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn tick<S>(&mut self, spawner: S) -> Result<bool, RunnerError>
    where
        S: Spawn,
    {
        let mut did_work = false;

        // Process all events
        let (processed, commands) = self.core.process_events();

        if processed {
            did_work = true;
            if self.config.debug_logging {
                println!("Runner: Processing {} commands", commands.len());
            }

            // Dispatch commands to Shell (may route events back to Core)
            for command in commands {
                self.shell.dispatch(command).map_err(RunnerError::Shell)?;
            }
        }

        // 4. Process effects in Shell (now async for sequential effect processing)
        let shell_work = self.shell.tick(spawner).await.map_err(RunnerError::Shell)?;
        if shell_work {
            did_work = true;
            if self.config.debug_logging {
                println!("Runner: Shell processed effects");
            }
        }

        Ok(did_work)
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
    pub fn shell(&self) -> &Shell<Event, Effect, Resources, Executors, H> {
        &self.shell
    }

    /// Get a mutable reference to the Shell
    pub fn shell_mut(&mut self) -> &mut Shell<Event, Effect, Resources, Executors, H> {
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
    pub fn shutdown(&mut self) {
        self.shell.shutdown();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::storage::{EmptyStorage, Storage};

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
        ctx: &mut crate::event_context::EventContext<
            TestEvent,
            TestEffect,
            Storage<TestModel, EmptyStorage>,
        >,
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
            .effect_handler(|_e: TestEffect, _ctx| async { crate::streaming::EffectOutput::None })
            .build();

        let event_sender = core.event_sender();

        let mut runner = Runner::new(core, shell);

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Process one tick
        let did_work = runner.tick(crate::spawn::TokioSpawn).await.unwrap();

        assert!(did_work);
        let model: &TestModel = runner.core().storage().get();
        assert_eq!(model.count, 1); // Ping -> count=1, Pong event dispatched but not processed yet in same tick
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_runner_until_condition() {
        let (core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| async { crate::streaming::EffectOutput::None })
            .build();

        let event_sender = core.event_sender();

        let mut runner = Runner::with_config(
            core,
            shell,
            RunnerConfig {
                idle_sleep: Duration::from_millis(1),
                max_run_duration: Some(Duration::from_secs(1)),
                debug_logging: false,
                runtime: crate::timer::time(),
            },
        );

        // Send multiple events
        for _ in 0..5 {
            event_sender.send(TestEvent::Ping).unwrap();
        }

        // Run until count reaches 10
        runner
            .run_until(
                |core, _shell| {
                    let model: &TestModel = core.storage().get();
                    model.count >= 10
                },
                crate::spawn::TokioSpawn,
            )
            .await
            .unwrap();

        let model: &TestModel = runner.core().storage().get();
        assert_eq!(model.count, 10);
    }
}
