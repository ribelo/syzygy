//! # Shell - Asynchronous Effect Management
//!
//! The Shell orchestrates asynchronous effect execution, bridging pure Core updates
//! with side-effectful operations. Effect handlers now return `Task` plans which
//! the Shell drives using executors registered in an immutable registry.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::activity::Activity;
use crate::command::{Command, CommandStep};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::task::{drive_task_with_activity, PanicHook};
use crate::executor::{ExecutorRegistry, Task};

#[cfg(feature = "tracing")]
use tracing::{debug, span, Level};

use crate::extract::EffectContext;

pub(crate) type EffectHandlerFn<E, X, R> =
    Box<dyn Fn(X, &EffectContext<R>) -> Task<E, X> + Send + 'static>;

#[derive(Clone, Default)]
pub struct ShellStats {
    inner: Arc<ShellStatsInner>,
}

struct ShellStatsInner {
    dropped_events: AtomicUsize,
    dropped_effect_steps: AtomicUsize,
}

impl Default for ShellStatsInner {
    fn default() -> Self {
        Self {
            dropped_events: AtomicUsize::new(0),
            dropped_effect_steps: AtomicUsize::new(0),
        }
    }
}

impl ShellStats {
    pub fn inc_dropped_event(&self) {
        self.inner.dropped_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_dropped_effect_step(&self) {
        self.inner
            .dropped_effect_steps
            .fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn snapshot(&self) -> ShellStatsSnapshot {
        ShellStatsSnapshot {
            dropped_events: self.inner.dropped_events.load(Ordering::Relaxed),
            dropped_effect_steps: self.inner.dropped_effect_steps.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellStatsSnapshot {
    pub dropped_events: usize,
    pub dropped_effect_steps: usize,
}

/// The Shell orchestrates async effect execution independently of Core
pub struct Shell<E, X, R = ()>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    /// Channel for receiving command outputs (effects)
    pub(crate) effect_rx: Receiver<CommandStep<E, X>>,
    pub(crate) effect_tx: Sender<CommandStep<E, X>>,

    /// Channel for sending events back to Core
    pub(crate) event_tx: EventSender<E>,

    /// Registry of pluggable executors
    pub(crate) executors: Arc<ExecutorRegistry<E>>,

    /// User-provided effect handler
    pub(crate) effect_handler: EffectHandlerFn<E, X, R>,

    /// Shared application resources cloned per effect invocation
    pub(crate) resources: R,

    /// Activity tracker for in-flight async work
    pub(crate) activity: Activity,

    /// Optional capacity for the effect queue (None => unbounded)
    pub(crate) effect_channel_capacity: Option<usize>,

    /// Closed flag for Runner shutdown checks
    pub(crate) closed: bool,

    /// Prefetched effects waiting to be processed (local overflow buffer)
    pub(crate) prefetched_effects: VecDeque<CommandStep<E, X>>,

    /// Observability stats for dropped work
    pub(crate) stats: ShellStats,

    /// Tracks whether executor shutdown has been requested
    pub(crate) executors_shutdown: bool,

    /// Optional panic handler invoked when tasks panic.
    pub(crate) panic_handler: Option<Arc<PanicHook<E, X>>>,
}

// No public constructors. Shell instances are created exclusively by the builder.

impl<E, X, R> Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    fn push_effect_step(&mut self, step: CommandStep<E, X>) -> Result<(), ShellError> {
        match self.effect_tx.try_send(step) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(step)) => {
                if let Ok(queued_step) = self.effect_rx.try_recv() {
                    self.prefetched_effects.push_back(queued_step);
                }

                let limit = self.effect_channel_capacity.unwrap_or(usize::MAX);
                let occupancy = self.effect_rx.len() + self.prefetched_effects.len();
                if occupancy >= limit {
                    return Err(ShellError::EffectQueueFull { capacity: limit });
                }

                match self.effect_tx.try_send(step) {
                    Ok(()) => Ok(()),
                    Err(TrySendError::Full(step)) => {
                        let occupancy = self.effect_rx.len() + self.prefetched_effects.len();
                        if occupancy < limit {
                            self.prefetched_effects.push_back(step);
                            Ok(())
                        } else {
                            Err(ShellError::EffectQueueFull { capacity: limit })
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        self.stats.inc_dropped_effect_step();
                        Err(ShellError::CommandExecutionFailed(
                            "Effect channel closed".to_string(),
                        ))
                    }
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                self.stats.inc_dropped_effect_step();
                Err(ShellError::CommandExecutionFailed(
                    "Effect channel closed".to_string(),
                ))
            }
        }
    }

    fn process_effect(&mut self, effect: X) -> Result<(), ShellError> {
        let ctx = EffectContext::new(self.resources.clone());
        let task = (self.effect_handler)(effect, &ctx);
        drive_task_with_activity(
            &self.executors,
            task,
            self.event_tx.clone(),
            self.effect_tx.clone(),
            Some(&self.activity),
            Some(&self.stats),
            self.panic_handler.as_ref(),
        )
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
                        self.stats.inc_dropped_event();
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

    fn handle_effect_step(&mut self, step: CommandStep<E, X>) -> Result<usize, ShellError> {
        match step {
            CommandStep::Effect(effect) => {
                self.process_effect(effect)?;
                Ok(1)
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                let mut handled = 0usize;
                for effect in effects {
                    self.process_effect(effect)?;
                    handled += 1;
                }
                Ok(handled)
            }
            CommandStep::Event(_) => unreachable!(),
        }
    }

    /// Process all pending effects synchronously and return count of effects processed
    pub fn drain(&mut self) -> Result<usize, ShellError> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "shell_drain").entered();

        let mut effect_count = 0usize;

        while let Some(step) = self.next_effect_step() {
            effect_count += self.handle_effect_step(step)?;
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
    pub fn poll_one(&mut self) -> Result<bool, ShellError> {
        if let Some(step) = self.next_effect_step() {
            self.handle_effect_step(step)?;
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
    /// without actually processing them. Value is a snapshot and may become
    /// stale immediately due to concurrent producers.
    #[must_use]
    pub fn pending_effects(&self) -> usize {
        self.effect_rx.len() + self.prefetched_effects.len()
    }

    /// Get the number of currently in-flight async jobs
    ///
    /// This includes async futures, streams, and blocking jobs that have been
    /// spawned but not yet completed.
    #[must_use]
    pub fn inflight_jobs(&self) -> usize {
        self.activity.load()
    }

    /// Check if the shell is completely idle
    ///
    /// Returns true if there are no pending effects and no in-flight jobs.
    /// This is the authoritative check for determining if all async work has completed.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.pending_effects() == 0 && self.inflight_jobs() == 0
    }

    /// Check if the shell is closed
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Get the configured effect channel capacity (None => unbounded)
    #[must_use]
    pub fn effect_channel_capacity(&self) -> Option<usize> {
        self.effect_channel_capacity
    }

    /// Override the effect channel capacity (None => unbounded)
    pub fn set_effect_channel_capacity(&mut self, capacity: Option<usize>) {
        self.effect_channel_capacity = capacity;
    }

    /// Signal shutdown to higher-level Runner logic and request executor shutdown.
    pub fn shutdown(&mut self) {
        self.closed = true;
        if !self.executors_shutdown {
            self.executors.shutdown_all();
            self.executors_shutdown = true;
        }
    }

    /// Wait for all registered executors to finish outstanding work.
    pub fn wait_for_executors(&self) {
        self.executors.wait_all();
    }

    #[must_use]
    pub fn stats(&self) -> ShellStatsSnapshot {
        self.stats.snapshot()
    }

    #[must_use]
    pub fn stats_handle(&self) -> ShellStats {
        self.stats.clone()
    }

    #[must_use]
    pub fn panic_handler(&self) -> Option<&Arc<PanicHook<E, X>>> {
        self.panic_handler.as_ref()
    }
}

impl<E, X, R> Drop for Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    fn drop(&mut self) {
        self.shutdown();
        self.wait_for_executors();

        #[cfg(feature = "tracing")]
        {
            let snapshot = self.stats();
            if snapshot.dropped_events > 0 || snapshot.dropped_effect_steps > 0 {
                tracing::warn!(
                    stage = "shell_drop",
                    dropped_events = snapshot.dropped_events,
                    dropped_effect_steps = snapshot.dropped_effect_steps,
                    "Shell dropped work during shutdown — events were still in flight. Consider calling `await_idle`, draining the runner, or increasing queue capacities."
                );
            } else {
                tracing::debug!(stage = "shell_drop", "Shell shutdown cleanly");
            }
        }
    }
}

impl<E, X, R> std::fmt::Debug for Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("pending_effects", &"<pending>")
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
