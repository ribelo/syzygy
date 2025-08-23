use crossbeam_channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::App;
use crate::async_context::EffectContext;
use crate::command::{Command, CommandStep};
use crate::command::executor::route_command;
use crate::error::ShellError;
use crate::task::{TaskStats, TaskTracker};
use crate::timer::{Time, time};
 
use crate::effect_handler::EffectHandler;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span, warn};

/// Shell is generic over the effect handler `H` implementing AFIT for zero allocations
///
/// Configuration for Shell effect execution
pub struct ShellConfig {
    /// Timeout for individual effect execution
    pub effect_timeout: Option<Duration>,
    /// Runtime implementation for timeout handling - provides runtime neutrality
    pub runtime: Time,
    /// Optional callback to handle timeout events
    ///
    /// When provided, this function will be called when an effect execution times out.
    /// This allows applications to handle timeouts with custom logic (logging, metrics, etc.)
    /// beyond the default stderr logging.
    ///
    /// The callback receives no parameters and should perform any needed side effects
    /// directly (e.g., increment counters, send alerts). If you need to send events
    /// back to Core on timeout, handle that in your effect handler by checking for
    /// timeout errors.
    ///
    /// If None, timeouts are only logged to stderr.
    pub on_timeout_callback: Option<Arc<dyn Fn() + Send + Sync>>,

    /// Channel capacity for effect queue
    ///
    /// When None (default), uses unbounded channels for maximum throughput and simplicity.
    /// Unbounded channels are appropriate for most applications because:
    ///
    /// - Effects are processed quickly by design (they describe work, don't do it)
    /// - Backpressure at the effect level would block the entire event loop
    /// - Memory usage is typically dominated by the work being described, not the descriptions
    ///
    /// When Some(capacity), uses bounded channels with the specified capacity.
    /// This can be useful for:
    ///
    /// - Applications with strict memory constraints
    /// - Debug builds that want to catch runaway effect generation
    /// - Integration with external systems that need backpressure
    ///
    /// **Note**: Bounded channels can cause `Shell::dispatch()` to block
    /// if the effect queue fills up, potentially deadlocking the application.
    /// Use with caution.
    pub effect_channel_capacity: Option<usize>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            effect_timeout: Some(Duration::from_secs(30)), // 30 second timeout
            runtime: time(),
            on_timeout_callback: None, // No custom timeout handling by default
            effect_channel_capacity: None, // Unbounded channels by default
        }
    }
}

/// The Shell orchestrates async effect execution independently of Core
///
/// Shell is responsible for:
/// - Executing Commands and their effects
/// - Managing async tasks with TaskTracker
/// - Bridging sync Core with async world
/// - Routing events back to Core from command execution
/// - Providing Resources to effect handlers
///
/// The Shell works alongside Core rather than owning it, giving users
/// maximum flexibility in how they structure their applications.
pub struct Shell<A: App, H = ()> {
    /// Task tracker for managing async effects (wrapped for sharing with EffectContext)
    task_tracker: Arc<Mutex<TaskTracker>>,

    /// Channel for receiving command outputs (effects)
    effect_rx: Receiver<CommandStep<A::Event, A::Effect>>,
    effect_tx: Sender<CommandStep<A::Event, A::Effect>>,

    /// Channel for sending events back to Core
    event_tx: Option<Sender<A::Event>>,

    /// Resources shared with effect handlers
    resources: Option<Arc<A::Resources>>,

    /// User-provided effect handler (AFIT, zero-alloc). Default is `()` which is a noop handler.
    effect_handler: H,

    /// Configuration for error handling and timeouts
    config: ShellConfig,
}

impl<A: App> Default for Shell<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: App> Shell<A, ()> {
    /// Create a new Shell with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ShellConfig::default())
    }

    /// Create a new Shell with custom configuration
    #[must_use]
    pub fn with_config(config: ShellConfig) -> Self {
        // Create channel based on configuration
        let (effect_tx, effect_rx) = match config.effect_channel_capacity {
            Some(capacity) => crossbeam_channel::bounded(capacity),
            None => crossbeam_channel::unbounded(),
        };

        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            effect_rx,
            effect_tx,
            event_tx: None,
            resources: None,
            effect_handler: (),
            config,
        }
    }
}

impl<A: App, H> Shell<A, H> {
    /// Create a new Shell with custom configuration and explicit handler
    #[must_use]
    pub fn with_config_and_handler(config: ShellConfig, handler: H) -> Self {
        // Create channel based on configuration
        let (effect_tx, effect_rx) = match config.effect_channel_capacity {
            Some(capacity) => crossbeam_channel::bounded(capacity),
            None => crossbeam_channel::unbounded(),
        };

        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            effect_rx,
            effect_tx,
            event_tx: None,
            resources: None,
            effect_handler: handler,
            config,
        }
    }

    /// Replace the effect handler with a new AFIT handler (type-changing method)
    #[must_use]
    pub fn with_effect_handler<H2>(self, handler: H2) -> Shell<A, H2> {
        Shell {
            task_tracker: self.task_tracker,
            effect_rx: self.effect_rx,
            effect_tx: self.effect_tx,
            event_tx: self.event_tx,
            resources: self.resources,
            effect_handler: handler,
            config: self.config,
        }
    }

    /// Set the resources for effect handlers
    #[must_use]
    pub fn with_resources(mut self, resources: A::Resources) -> Self {
        self.resources = Some(Arc::new(resources));
        self
    }

    /// Set the event sender for routing events back to Core
    ///
    /// This allows Commands executed by Shell to send events back to Core for processing.
    #[must_use]
    pub fn with_event_sender(mut self, event_sender: Sender<A::Event>) -> Self {
        self.event_tx = Some(event_sender);
        self
    }

    /// Get an effect sender for executing effects
    #[must_use]
    pub fn effect_sender(&self) -> Sender<CommandStep<A::Event, A::Effect>> {
        self.effect_tx.clone()
    }

    /// Execute a Command following Crux's pattern
    ///
    /// This processes the Command synchronously and routes outputs:
    /// - Effects go to the effect handler via effect channel
    /// - Events go back to Core via event channel
    ///
    /// Command processing is now synchronous - async effect execution happens in tick().
    pub fn dispatch(
        &mut self,
        command: Command<A::Event, A::Effect>,
    ) -> Result<(), ShellError>
    where
        A::Event: Clone + Send + 'static,
        A::Effect: Clone + Send + 'static,
    {
        #[cfg(feature = "tracing")]
        debug!("Executing command");

        let effect_sender = &self.effect_tx;
        let event_sender = self.event_tx.as_ref();

        // Execute command directly to immediately route events back to Core
        // This ensures events are processed in the same step
        if let Err(e) = route_command(command, event_sender, effect_sender) {
            #[cfg(feature = "tracing")]
            warn!("Command execution failed: {:?}", e);
            eprintln!("Command execution failed: {e:?}");
            return Err(ShellError::CommandExecutionFailed(e.to_string()));
        }

        #[cfg(feature = "tracing")]
        debug!("Command executed directly");

        Ok(())
    }

    /// Process effects and manage async tasks
    ///
    /// This processes any pending effects with timeout and panic handling.
    /// Returns true if there was work to do, false if idle.
    pub fn tick<S>(&mut self, spawner: S) -> Result<bool, ShellError>
    where
        S: crate::spawn::Spawn,
        H: EffectHandler<A::Event, A::Effect, A::Resources> + Clone + Send + Sync + 'static,
        A::Event: Clone + Send + 'static,
        A::Effect: Clone + Send + 'static,
    {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "shell_tick").entered();

        let mut did_work = false;
        let mut _effect_count = 0;

        // Process any command outputs that were sent to us
        while let Ok(command_output) = self.effect_rx.try_recv() {
            match command_output {
                CommandStep::Effect(effect) => {
                    did_work = true;
                    _effect_count += 1;

                    #[cfg(feature = "tracing")]
                    debug!("Processing effect");

                    if let Some(resources) = &self.resources {
                        let resources = Arc::clone(resources);
                        let ctx = EffectContext::with_runtime(self.event_tx.clone(), self.config.runtime);
                        let handler = self.effect_handler.clone();
                        let runtime = self.config.runtime;
                        let timeout = self.config.effect_timeout;

                        match timeout {
                            Some(dur) => {
                                spawner.spawn(async move {
                                    let fut = handler.handle(effect, resources, ctx);
                                    let _ = runtime.timeout(dur, Box::pin(fut)).await;
                                });
                            }
                            None => {
                                spawner.spawn(async move {
                                    handler.handle(effect, resources, ctx).await;
                                });
                            }
                        }
                    }
                }
                CommandStep::InlineFuture(closure) => {
                    did_work = true;
                    _effect_count += 1;

                    let event_tx = self.event_tx.clone();
                    let runtime = self.config.runtime;
                    let timeout = self.config.effect_timeout;
                    let fut = closure;

                    // Build a future that runs the inline task with timeout handling
                    let inline = async move {
                        let ctx = EffectContext::with_runtime(event_tx, runtime);
                        match timeout {
                            Some(dur) => {
                                let _ = runtime.timeout(dur, fut(ctx)).await;
                            }
                            None => {
                                fut(ctx).await;
                            }
                        }
                    };
                    spawner.spawn(inline);
                }
                CommandStep::SequentialEffects(effects) => {
                    did_work = true;
                    _effect_count += effects.len();

                    if let Some(resources) = &self.resources {
                        let resources = Arc::clone(resources);
                        let event_tx = self.event_tx.clone();
                        let runtime = self.config.runtime;
                        let timeout = self.config.effect_timeout;
                        let handler = self.effect_handler.clone();

                        let seq_future = async move {
                            let ctx = EffectContext::with_runtime(event_tx, runtime);
                            for effect in effects {
                                match timeout {
                                    Some(dur) => {
                                        let _ = runtime
                                            .timeout(dur, Box::pin(handler.handle(effect, Arc::clone(&resources), ctx.clone())))
                                            .await;
                                    }
                                    None => {
                                        handler.handle(effect, Arc::clone(&resources), ctx.clone()).await;
                                    }
                                }
                            }
                        };
                        spawner.spawn(seq_future);
                    }
                }
                CommandStep::ParallelEffects(effects) => {
                    did_work = true;
                    _effect_count += effects.len();

                    if let Some(resources) = &self.resources {
                        let resources = Arc::clone(resources);
                        let event_tx = self.event_tx.clone();
                        let runtime = self.config.runtime;
                        let timeout = self.config.effect_timeout;
                        let handler = self.effect_handler.clone();

                        for effect in effects {
                            let resources = Arc::clone(&resources);
                            let ctx = EffectContext::with_runtime(event_tx.clone(), runtime);
                            match timeout {
                                Some(dur) => {
                                    let h = handler.clone();
                                    spawner.spawn(async move {
                                        let _ = runtime.timeout(dur, Box::pin(h.handle(effect, resources, ctx))).await;
                                    });
                                }
                                None => {
                                    let h = handler.clone();
                                    spawner.spawn(async move {
                                        h.handle(effect, resources, ctx).await;
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Other command outputs should be handled by Core
                    // This shouldn't happen in normal operation
                }
            }
        }

        // Cleanup finished tasks
        let (_cleaned_count, _active_count) = if let Ok(mut tracker) = self.task_tracker.lock() {
            let cleaned = tracker.active_count();
            tracker.cleanup_finished();
            let active = tracker.active_count();
            (cleaned, active)
        } else {
            (0, 0)
        };

        #[cfg(feature = "tracing")]
        if did_work {
            debug!(
                _effect_count,
                active_tasks = _active_count,
                cleaned_tasks = _cleaned_count - _active_count,
                "Shell tick completed"
            );
        }

        Ok(did_work)
    }

    // Removed: boxed protected future path; handled inline to avoid allocations in common case

    /// Get task tracker statistics
    #[must_use]
    pub fn task_stats(&self) -> TaskStats {
        if let Ok(tracker) = self.task_tracker.lock() {
            TaskStats {
                active_tasks: tracker.active_count(),
                is_closed: tracker.is_closed(),
            }
        } else {
            TaskStats {
                active_tasks: 0,
                is_closed: true,
            }
        }
    }

    /// Get the current shell configuration
    #[must_use]
    pub fn config(&self) -> &ShellConfig {
        &self.config
    }

    /// Update the shell configuration
    pub fn set_config(&mut self, config: ShellConfig) {
        self.config = config;
    }

    /// Shutdown the shell by closing the task tracker
    ///
    /// **Important**: This method only closes the task tracker to prevent new tasks
    /// from being spawned. It does NOT wait for existing tasks to complete or cancel them.
    ///
    /// ## Shutdown Behavior
    ///
    /// - **Immediate**: Prevents new effects from being executed
    /// - **Non-blocking**: Returns immediately without waiting for tasks
    /// - **No cancellation**: Existing tasks continue running until completion
    ///
    /// ## Recommended Shutdown Pattern
    ///
    /// For graceful shutdown with task completion:
    ///
    /// ```rust,ignore
    /// use std::time::Duration;
    ///
    /// // 1. Signal shutdown intent (stop sending new events)
    /// shell.shutdown();
    ///
    /// // 2. Wait for pending tasks to complete (if needed) - in async context
    /// while shell.task_stats().active_tasks > 0 {
    ///     tokio::time::sleep(Duration::from_millis(10)).await;
    /// }
    ///
    /// // 3. Tasks complete naturally or reach their own timeouts
    /// ```
    ///
    /// For immediate shutdown with task cancellation, use EffectContext's
    /// drop-based cancellation by ensuring all EffectContext instances are dropped.
    pub fn shutdown(&mut self) {
        if let Ok(tracker) = self.task_tracker.lock() {
            tracker.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    #[derive(Debug, Default)]
    struct TestApp;

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Dummy, // Just for the shell test - not actually used
    }

    #[derive(Debug, Default)]
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

        fn update(
            &self,
            event: Self::Event,
            model: &mut Self::Model,
        ) -> Command<Self::Event, Self::Effect> {
            match event {
                TestEvent::Dummy => {
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

    #[test]
    fn test_shell_creation() {
        let shell = Shell::<TestApp>::new();

        assert_eq!(shell.task_stats().active_tasks, 0);
        assert!(!shell.task_stats().is_closed);
    }
}
