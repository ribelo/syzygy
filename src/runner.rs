use std::time::Duration;

use crate::app::App;
use crate::core::Core;
use crate::shell::Shell;
use crate::effect_handler::EffectHandler;
use crate::spawn::Spawn;
use crate::error::{ShellError, CoreError};
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
pub struct Runner<A: App, H = ()> {
    core: Core<A>,
    shell: Shell<A, H>,
    config: RunnerConfig,
}

impl<A: App, H> Runner<A, H> {
    /// Create a new Runner with Core and Shell
    pub fn new(core: Core<A>, shell: Shell<A, H>) -> Self {
        Self {
            core,
            shell,
            config: RunnerConfig::default(),
        }
    }

    /// Create a new Runner with custom configuration
    pub fn with_config(core: Core<A>, shell: Shell<A, H>, config: RunnerConfig) -> Self {
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
        A::Event: Clone + Send + 'static,
        A::Effect: Clone + Send + 'static,
        H: EffectHandler<A::Event, A::Effect, A::Resources> + Clone + Send + Sync + 'static,
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
    pub async fn run_until<F, S>(
        &mut self,
        mut condition: F,
        spawner: S
    ) -> Result<(), RunnerError>
    where
        F: FnMut(&Core<A>, &Shell<A, H>) -> bool,
        A::Event: Clone + Send + 'static,
        A::Effect: Clone + Send + 'static,
        H: EffectHandler<A::Event, A::Effect, A::Resources> + Clone + Send + Sync + 'static,
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
                && start_time.elapsed() > max_duration {
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
    pub async fn tick<S>(&mut self, spawner: S) -> Result<bool, RunnerError>
    where
        A::Event: Clone + Send + 'static,
        A::Effect: Clone + Send + 'static,
        H: EffectHandler<A::Event, A::Effect, A::Resources> + Clone + Send + Sync + 'static,
        S: Spawn,
    {
        let mut did_work = false;

        // Only poll external events initially if we have no queued work
        if !self.core.has_queued_events() {
            self.core.poll_external_events();
        }

        // Process all events until stable (commands may generate more events)
        loop {
            // 1. Process any events in the queue
            let commands = self.core.process_queued_events();
            if commands.is_empty() {
                break; // No more events to process
            }

            did_work = true;
            if self.config.debug_logging {
                println!("Runner: Processing {} commands", commands.len());
            }

            // 3. Dispatch commands to Shell (may route events back to Core)
            for command in commands {
                self.shell.dispatch(command).map_err(RunnerError::Shell)?;
            }

            // Only poll external events between batches if we don't have queued work
            if !self.core.has_queued_events() {
                self.core.poll_external_events();
            }
        }

        // 4. Process effects in Shell
        let shell_work = self.shell.tick(spawner)
            .map_err(RunnerError::Shell)?;
        if shell_work {
            did_work = true;
            if self.config.debug_logging {
                println!("Runner: Shell processed effects");
            }
        }

        Ok(did_work)
    }

    /// Get a reference to the Core
    pub fn core(&self) -> &Core<A> {
        &self.core
    }

    /// Get a mutable reference to the Core
    pub fn core_mut(&mut self) -> &mut Core<A> {
        &mut self.core
    }

    /// Get a reference to the Shell
    pub fn shell(&self) -> &Shell<A, H> {
        &self.shell
    }

    /// Get a mutable reference to the Shell
    pub fn shell_mut(&mut self) -> &mut Shell<A, H> {
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

    #[derive(Debug)]
    struct TestApp;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Ping,
        Pong,
    }

    #[derive(Debug)]
    struct TestModel {
        count: i32,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    impl App for TestApp {
        type Event = TestEvent;
        type Model = TestModel;
        #[cfg(feature = "view-model")]
        type ViewModel = i32;
        type Effect = TestEffect;
        type Resources = ();

        fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
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

        #[cfg(feature = "view-model")]
        fn view(&self, model: &Self::Model) -> Self::ViewModel {
            model.count
        }
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_runner_basic() {
        let (core, shell) = Syzygy::builder::<TestApp>()
            .app(TestApp)
            .model(TestModel { count: 0 })
            .build();

        let event_sender = core.event_sender();

        let mut runner = Runner::new(core, shell);

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Process one tick
        let did_work = runner.tick(crate::spawn::TokioSpawn).await.unwrap();

        assert!(did_work);
        assert_eq!(runner.core().model().count, 2); // Ping -> Pong -> +2
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn test_runner_until_condition() {
        let (core, shell) = Syzygy::builder::<TestApp>()
            .app(TestApp)
            .model(TestModel { count: 0 })
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
            }
        );

        // Send multiple events
        for _ in 0..5 {
            event_sender.send(TestEvent::Ping).unwrap();
        }

        // Run until count reaches 10
        runner.run_until(
            |core, _shell| core.model().count >= 10,
            crate::spawn::TokioSpawn
        ).await.unwrap();

        assert_eq!(runner.core().model().count, 10);
    }
}
