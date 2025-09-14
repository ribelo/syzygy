//! # Shell - Asynchronous Effect Management
//!
//! The Shell orchestrates asynchronous effect execution, bridging pure Core updates
//! with side-effectful operations. Effect handlers return `EffectSpec` plans which
//! the Shell drives using executors registered in an immutable registry.

use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crate::command::{Command, CommandStep};
use crate::effect_context::EffectContext;
use crate::error::ShellError;
use crate::executor::ExecutorRegistry;
use crate::executor::spec::drive_spec;

use crate::timer::{Time, time};

/// Sync effect handler producing an `Task` plan.
pub type EffectHandler<E, X, R> = fn(X, &EffectContext<E, R>) -> crate::executor::Task<E, R>;
// use futures_util::future::*;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span};

/// Configuration for Shell effect execution
pub struct ShellConfig {
    /// Timeout for individual effect execution (reserved; not enforced at Shell level)
    pub effect_timeout: Option<Duration>,
    /// Runtime implementation for timeout handling - provides runtime neutrality
    pub runtime: Time,
    /// Optional callback to handle timeout events
    pub on_timeout_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Capacity for effect queue (None => unbounded)
    pub effect_channel_capacity: Option<usize>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            effect_timeout: Some(Duration::from_secs(30)),
            runtime: time(),
            on_timeout_callback: None,
            effect_channel_capacity: None,
        }
    }
}

/// The Shell orchestrates async effect execution independently of Core
pub struct Shell<E, X, R = ()>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    /// Channel for receiving command outputs (effects)
    pub(crate) effect_rx: Receiver<CommandStep<E, X>>,
    pub(crate) effect_tx: Sender<CommandStep<E, X>>,

    /// Channel for sending events back to Core
    pub(crate) event_tx: Sender<E>,

    /// Resources shared with effect handlers (user controls Arc wrapping)
    pub(crate) resources: R,

    /// Registry of pluggable executors
    pub(crate) executors: Arc<ExecutorRegistry<E>>,

    /// User-provided effect handler
    pub(crate) effect_handler: EffectHandler<E, X, R>,

    /// Configuration for error handling and timeouts
    pub(crate) config: ShellConfig,

    /// Closed flag for Runner shutdown checks
    pub(crate) closed: bool,
}

// No public constructors. Shell instances are created exclusively by the builder.

impl<E, X, R> Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    /// Get an immutable reference to resources
    #[must_use]
    pub fn resource(&self) -> &R {
        &self.resources
    }

    // No public effect sender accessors to keep the API minimal.

    /// Process a Command synchronously and enqueue its outputs
    ///
    /// This method is crate-only to enforce TEA boundaries.
    /// Commands should be enqueued through the Runner, not directly.
    pub(crate) fn enqueue_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        #[cfg(feature = "tracing")]
        debug!("Executing command");

        let effect_sender = &self.effect_tx;
        let event_sender = &self.event_tx;

        #[cfg(feature = "tracing")]
        debug!("Starting synchronous command routing");

        let mut _event_count = 0;
        let mut _effect_count = 0;

        // Simple synchronous iteration over command outputs
        for output in command {
            match output {
                CommandStep::Event(event) => {
                    _event_count += 1;
                    #[cfg(feature = "tracing")]
                    debug!("Routing event to Core");

                    event_sender.send(event).map_err(|_| {
                        ShellError::CommandExecutionFailed("Event channel closed".to_string())
                    })?;
                }
                CommandStep::Effect(effect) => {
                    _effect_count += 1;
                    #[cfg(feature = "tracing")]
                    debug!("Routing single effect to Shell");

                    effect_sender
                        .send(CommandStep::Effect(effect))
                        .map_err(|_| {
                            ShellError::CommandExecutionFailed("Effect channel closed".to_string())
                        })?;
                }
                CommandStep::Batch(effects) => {
                    _effect_count += effects.len();
                    #[cfg(feature = "tracing")]
                    debug!(count = effects.len(), "Routing batch effects to Shell");

                    // Send the entire batch pattern to Shell for proper coordination
                    effect_sender
                        .send(CommandStep::Batch(effects))
                        .map_err(|_| {
                            ShellError::CommandExecutionFailed("Effect channel closed".to_string())
                        })?;
                } // No other variants
            }
        }

        #[cfg(feature = "tracing")]
        debug!(
            _event_count,
            _effect_count, "Synchronous command execution completed"
        );

        Ok(())
    }

    /// Process all pending effects synchronously and return count of effects processed
    pub fn drain_with<S>(&mut self, scheduler: S) -> Result<usize, ShellError>
    where
        S: crate::scheduler::Scheduler,
    {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "shell_drain").entered();

        let mut effect_count = 0usize;
        let scheduler = scheduler; // Clone for use in async closures

        while let Ok(step) = self.effect_rx.try_recv() {
            match step {
                CommandStep::Effect(fx) => {
                    effect_count += 1;
                    let ctx =
                        EffectContext::new(self.resources.clone(), Arc::clone(&self.executors));
                    let spec = (self.effect_handler)(fx, &ctx);
                    scheduler.schedule(drive_spec(
                        spec,
                        ctx,
                        self.event_tx.clone(),
                        scheduler.clone(),
                    ));
                }
                CommandStep::Batch(effects) => {
                    effect_count += effects.len();
                    let ctx =
                        EffectContext::new(self.resources.clone(), Arc::clone(&self.executors));
                    let handler = self.effect_handler;
                    let event_tx = self.event_tx.clone();
                    let scheduler_clone = scheduler.clone();
                    let scheduler_for_async = scheduler_clone.clone();
                    scheduler.schedule(async move {
                        for fx in effects {
                            let spec = handler(fx, &ctx);
                            drive_spec(
                                spec,
                                ctx.clone(),
                                event_tx.clone(),
                                scheduler_for_async.clone(),
                            )
                            .await;
                        }
                    });
                }
                CommandStep::Event(_) => unreachable!(),
            }
        }

        #[cfg(feature = "tracing")]
        if effect_count > 0 {
            debug!(effects = effect_count, "Shell drain completed");
        }

        Ok(effect_count)
    }

    /// Process at most one effect from the queue synchronously
    ///
    /// Returns true if an effect was processed, false if the queue was empty
    pub fn poll_one_with<S>(&mut self, scheduler: S) -> Result<bool, ShellError>
    where
        S: crate::scheduler::Scheduler,
    {
        if let Ok(step) = self.effect_rx.try_recv() {
            match step {
                CommandStep::Effect(fx) => {
                    let ctx =
                        EffectContext::new(self.resources.clone(), Arc::clone(&self.executors));
                    let spec = (self.effect_handler)(fx, &ctx);
                    scheduler.schedule(drive_spec(
                        spec,
                        ctx,
                        self.event_tx.clone(),
                        scheduler.clone(),
                    ));
                }
                CommandStep::Batch(effects) => {
                    let ctx =
                        EffectContext::new(self.resources.clone(), Arc::clone(&self.executors));
                    let handler = self.effect_handler;
                    let event_tx = self.event_tx.clone();
                    let scheduler_clone = scheduler.clone();
                    scheduler.schedule(async move {
                        let sched = scheduler_clone;
                        for fx in effects {
                            let spec = handler(fx, &ctx);
                            drive_spec(spec, ctx.clone(), event_tx.clone(), sched.clone()).await;
                        }
                    });
                }
                CommandStep::Event(_) => unreachable!(),
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Process all pending effects using the default scheduler
    ///
    /// Returns the count of effects processed
    pub fn drain(&mut self) -> Result<usize, ShellError> {
        match crate::scheduler::scheduler_strict() {
            Ok(sched) => self.drain_with(sched),
            Err(e) => Err(ShellError::CommandExecutionFailed(format!(
                "No runtime available: {e}"
            ))),
        }
    }

    /// Process at most one effect using the default scheduler
    ///
    /// Returns true if an effect was processed, false if the queue was empty
    pub fn poll_one(&mut self) -> Result<bool, ShellError> {
        match crate::scheduler::scheduler_strict() {
            Ok(sched) => self.poll_one_with(sched),
            Err(e) => Err(ShellError::CommandExecutionFailed(format!(
                "No runtime available: {e}"
            ))),
        }
    }

    /// Get the number of pending effects in the queue
    ///
    /// This can be used to check if there are effects waiting to be processed
    /// without actually processing them.
    #[must_use]
    pub fn pending_effects(&self) -> usize {
        self.effect_rx.len()
    }

    /// Check if the shell is closed
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
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

    /// Signal shutdown to higher-level Runner logic
    pub fn shutdown(&mut self) {
        self.closed = true;
    }
}

impl<E, X, R> std::fmt::Debug for Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("resources", &"<resources>")
            .field("pending_effects", &"<pending>")
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
#[cfg(all(test, feature = "legacy_tests"))]
mod tests {

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Dummy,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEffect {
        Log,
    }
}
