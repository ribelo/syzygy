//! # Shell - Asynchronous Effect Management
//!
//! The Shell orchestrates asynchronous effect execution, bridging pure Core updates
//! with side-effectful operations. Effect handlers return `EffectSpec` plans which
//! the Shell drives using executors registered in an immutable registry.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use futures::future::BoxFuture;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use crate::command::{Command, CommandStep};
use crate::effect_context::EffectContext;
use crate::error::ShellError;
use crate::executor::spec::{Outcome, drive_spec};
use crate::executor::{ExecutorRegistry, Task};

/// Representation of the work produced by an effect handler.
pub enum EffectWork<E, R> {
    Task(Task<E, R>),
    Future(BoxFuture<'static, Outcome<E>>),
    Immediate(Outcome<E>),
}

pub trait IntoEffectWork<E, R> {
    fn into_effect_work(self) -> EffectWork<E, R>;
}

impl<E, R> IntoEffectWork<E, R> for EffectWork<E, R> {
    fn into_effect_work(self) -> EffectWork<E, R> {
        self
    }
}

impl<E, R> IntoEffectWork<E, R> for Task<E, R> {
    fn into_effect_work(self) -> EffectWork<E, R> {
        EffectWork::Task(self)
    }
}

impl<E, R> IntoEffectWork<E, R> for Outcome<E> {
    fn into_effect_work(self) -> EffectWork<E, R> {
        EffectWork::Immediate(self)
    }
}

impl<E, R, F> IntoEffectWork<E, R> for F
where
    F: Future<Output = Outcome<E>> + Send + 'static,
{
    fn into_effect_work(self) -> EffectWork<E, R> {
        EffectWork::Future(Box::pin(self))
    }
}

/// Effect handler accepting sync or async closures.
pub type EffectHandler<E, X, R> =
    Box<dyn FnMut(X, EffectContext<E, R>) -> EffectWork<E, R> + Send + 'static>;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span, warn};

/// Configuration for Shell effect execution
pub struct ShellConfig {
    /// Capacity for effect queue (None => unbounded, default: unbounded)
    pub effect_channel_capacity: Option<usize>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            effect_channel_capacity: None,
        }
    }
}

/// The Shell orchestrates async effect execution independently of Core
pub struct Shell<E, X, R = ()>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Send + Sync + 'static,
{
    /// Channel for receiving command outputs (effects)
    pub(crate) effect_rx: Receiver<CommandStep<E, X>>,
    pub(crate) effect_tx: Sender<CommandStep<E, X>>,

    /// Channel for sending events back to Core
    pub(crate) event_tx: Sender<E>,

    /// Resources shared with effect handlers (owned Arc)
    pub(crate) resources: Arc<R>,

    /// Registry of pluggable executors
    pub(crate) executors: Arc<ExecutorRegistry<E>>,

    /// User-provided effect handler
    pub(crate) effect_handler: EffectHandler<E, X, R>,

    /// Configuration for error handling and timeouts
    pub(crate) config: ShellConfig,

    /// Closed flag for Runner shutdown checks
    pub(crate) closed: bool,

    /// Prefetched effects waiting to be processed (local overflow buffer)
    pub(crate) prefetched_effects: VecDeque<CommandStep<E, X>>,
}

// No public constructors. Shell instances are created exclusively by the builder.

impl<E, X, R> Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Send + Sync + 'static,
{
    fn effect_context(&self) -> EffectContext<E, R> {
        EffectContext::new(Arc::clone(&self.resources), Arc::clone(&self.executors))
    }

    fn push_effect_step(&mut self, step: CommandStep<E, X>) -> Result<(), ShellError> {
        match self.effect_tx.try_send(step) {
            Ok(_) => Ok(()),
            Err(TrySendError::Full(step)) => {
                if let Ok(queued_step) = self.effect_rx.try_recv() {
                    self.prefetched_effects.push_back(queued_step);
                }

                match self.effect_tx.try_send(step) {
                    Ok(_) => Ok(()),
                    Err(TrySendError::Full(step)) => {
                        let limit = self.config.effect_channel_capacity.unwrap_or(usize::MAX);
                        if self.prefetched_effects.len() < limit {
                            self.prefetched_effects.push_back(step);
                            Ok(())
                        } else {
                            Err(ShellError::EffectQueueFull { capacity: limit })
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => Err(ShellError::CommandExecutionFailed(
                        "Effect channel closed".to_string(),
                    )),
                }
            }
            Err(TrySendError::Disconnected(_)) => Err(ShellError::CommandExecutionFailed(
                "Effect channel closed".to_string(),
            )),
        }
    }

    fn forward_outcome(event_tx: &Sender<E>, outcome: Outcome<E>) {
        match outcome {
            Outcome::None => {}
            Outcome::Event(event) => {
                let _ = event_tx.send(event);
            }
            Outcome::Events(events) => {
                for event in events {
                    let _ = event_tx.send(event);
                }
            }
        }
    }

    fn effect_future<S: crate::scheduler::Scheduler>(
        &self,
        work: EffectWork<E, R>,
        ctx: EffectContext<E, R>,
        scheduler: &S,
    ) -> BoxFuture<'static, ()> {
        match work {
            EffectWork::Task(task) => {
                let event_tx = self.event_tx.clone();
                let scheduler_clone = scheduler.clone();
                Box::pin(drive_spec(task, ctx, event_tx, scheduler_clone))
            }
            EffectWork::Future(fut) => {
                let event_tx = self.event_tx.clone();
                Box::pin(async move {
                    let outcome = fut.await;
                    Self::forward_outcome(&event_tx, outcome);
                })
            }
            EffectWork::Immediate(outcome) => {
                Self::forward_outcome(&self.event_tx, outcome);
                Box::pin(async move {})
            }
        }
    }

    fn process_effect<S: crate::scheduler::Scheduler>(
        &mut self,
        effect: X,
        scheduler: &S,
    ) -> BoxFuture<'static, ()> {
        let ctx = self.effect_context();
        let work = {
            let handler = &mut self.effect_handler;
            handler(effect, ctx.clone())
        };
        self.effect_future(work, ctx, scheduler)
    }

    /// Get an immutable reference to resources
    #[must_use]
    pub fn resource(&self) -> &R {
        self.resources.as_ref()
    }

    /// Clone the shared resource Arc when ownership is required
    #[must_use]
    pub fn resource_arc(&self) -> Arc<R> {
        Arc::clone(&self.resources)
    }

    // No public effect sender accessors to keep the API minimal.

    /// Route a command's outputs back into the effect and event queues.
    ///
    /// Useful when you want to orchestrate Core and Shell manually without the
    /// convenience Runner. Each command is processed synchronously.
    pub fn dispatch_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        #[cfg(feature = "tracing")]
        debug!("Executing command");

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

                    self.event_tx.send(event).map_err(|_| {
                        ShellError::CommandExecutionFailed("Event channel closed".to_string())
                    })?;
                }
                CommandStep::Effect(effect) => {
                    _effect_count += 1;
                    #[cfg(feature = "tracing")]
                    debug!("Routing single effect to Shell");

                    self.push_effect_step(CommandStep::Effect(effect))?;
                }
                CommandStep::Batch(effects) => {
                    _effect_count += effects.len();
                    #[cfg(feature = "tracing")]
                    debug!(count = effects.len(), "Routing batch effects to Shell");

                    self.push_effect_step(CommandStep::Batch(effects))?;
                }
                CommandStep::Parallel(effects) => {
                    _effect_count += effects.len();
                    #[cfg(feature = "tracing")]
                    debug!(count = effects.len(), "Routing parallel effects to Shell");

                    self.push_effect_step(CommandStep::Parallel(effects))?;
                }
            }
        }

        #[cfg(feature = "tracing")]
        debug!(
            _event_count,
            _effect_count, "Synchronous command execution completed"
        );

        Ok(())
    }

    fn next_effect_step(&mut self) -> Option<CommandStep<E, X>> {
        if let Some(step) = self.prefetched_effects.pop_front() {
            Some(step)
        } else {
            self.effect_rx.try_recv().ok()
        }
    }

    fn handle_effect_step<S: crate::scheduler::Scheduler>(
        &mut self,
        step: CommandStep<E, X>,
        scheduler: &S,
    ) {
        match step {
            CommandStep::Effect(effect) => {
                let fut = self.process_effect(effect, scheduler);
                scheduler.schedule(fut);
            }
            CommandStep::Batch(effects) => {
                if effects.is_empty() {
                    return;
                }

                let mut futures = Vec::with_capacity(effects.len());
                for effect in effects {
                    futures.push(self.process_effect(effect, scheduler));
                }

                let sequence = async move {
                    for fut in futures {
                        fut.await;
                    }
                };

                scheduler.schedule(sequence);
            }
            CommandStep::Parallel(effects) => {
                if effects.is_empty() {
                    return;
                }

                if scheduler.allows_overlap() {
                    for effect in effects {
                        let fut = self.process_effect(effect, scheduler);
                        scheduler.schedule(fut);
                    }
                    return;
                }

                #[cfg(feature = "tracing")]
                let effect_len = effects.len();

                #[cfg(feature = "tracing")]
                if effect_len > 1 {
                    warn!(
                        "Parallel command executed on non-overlapping scheduler; falling back to sequential execution"
                    );
                }

                let mut futures = Vec::with_capacity(effects.len());
                for effect in effects {
                    futures.push(self.process_effect(effect, scheduler));
                }

                let sequence = async move {
                    for fut in futures {
                        fut.await;
                    }
                };

                scheduler.schedule(sequence);
            }
            CommandStep::Event(_) => unreachable!(),
        }
    }

    /// Process all pending effects synchronously and return count of effects processed
    pub fn drain_with<S: crate::scheduler::Scheduler>(
        &mut self,
        scheduler: S,
    ) -> Result<usize, ShellError> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "shell_drain").entered();

        let mut effect_count = 0usize;
        let scheduler = scheduler;

        while let Some(step) = self.next_effect_step() {
            match &step {
                CommandStep::Effect(_) => effect_count += 1,
                CommandStep::Batch(effects) => effect_count += effects.len(),
                CommandStep::Parallel(effects) => effect_count += effects.len(),
                CommandStep::Event(_) => {}
            }
            self.handle_effect_step(step, &scheduler);
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
    pub fn poll_one_with<S: crate::scheduler::Scheduler>(
        &mut self,
        scheduler: S,
    ) -> Result<bool, ShellError> {
        if let Some(step) = self.next_effect_step() {
            self.handle_effect_step(step, &scheduler);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Block until an effect arrives or the timeout expires.
    ///
    /// Returns true if an effect was queued for processing.
    pub fn wait_for_effect(&mut self, timeout: Duration) -> bool {
        if self.closed {
            return false;
        }

        if !self.prefetched_effects.is_empty() || !self.effect_rx.is_empty() {
            return true;
        }

        if timeout.is_zero() {
            return false;
        }

        match self.effect_rx.recv_timeout(timeout) {
            Ok(step) => {
                self.prefetched_effects.push_back(step);
                true
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => {
                self.closed = true;
                false
            }
        }
    }

    /// Get the number of pending effects in the queue
    ///
    /// This can be used to check if there are effects waiting to be processed
    /// without actually processing them.
    #[must_use]
    pub fn pending_effects(&self) -> usize {
        self.effect_rx.len() + self.prefetched_effects.len()
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
