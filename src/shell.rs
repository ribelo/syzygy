//! # Shell - Asynchronous Effect Management
//!
//! The Shell orchestrates asynchronous effect execution, bridging pure Core updates
//! with side-effectful operations. Effect handlers return `Task` plans which
//! the Shell drives using configured async/compute/blocking executors.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::activity::Activity;
use crate::command::{CancelId, Command, CommandStep};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::task::{drive_task_with_activity, PanicHook, TaskRouting};
use crate::executor::{AsyncExecutor, BlockingExecutor, Task};

#[cfg(feature = "tracing")]
use tracing::{debug, span, Level};

use crate::extract::EffectContext;

pub(crate) type EffectHandlerFn<E, X, R> =
    Box<dyn Fn(X, &EffectContext<R>) -> Task<E, X> + Send + 'static>;

pub(crate) type CancelGenerationMap = Arc<Mutex<HashMap<CancelId, u64>>>;
pub(crate) type CancelGuard = Arc<dyn Fn() -> bool + Send + Sync + 'static>;

fn lock_cancel_generations(
    cancel_generations: &CancelGenerationMap,
) -> std::sync::MutexGuard<'_, HashMap<CancelId, u64>> {
    match cancel_generations.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(crate) fn register_cancellable_generation(
    cancel_generations: &CancelGenerationMap,
    id: CancelId,
) -> u64 {
    let mut generations = lock_cancel_generations(cancel_generations);
    let next = generations.get(&id).copied().unwrap_or(0).saturating_add(1);
    generations.insert(id, next);
    next
}

pub(crate) fn is_cancel_generation_current(
    cancel_generations: &CancelGenerationMap,
    id: CancelId,
    generation: u64,
) -> bool {
    let generations = lock_cancel_generations(cancel_generations);
    generations.get(&id).copied() == Some(generation)
}

pub(crate) fn prepare_effect_step_for_queue<E, X>(
    step: CommandStep<E, X>,
    cancel_generations: Option<&CancelGenerationMap>,
) -> CommandStep<E, X> {
    match (step, cancel_generations) {
        (
            CommandStep::CancellableEffect {
                id,
                effect,
                generation: 0,
            },
            Some(cancel_generations),
        ) => {
            let next = register_cancellable_generation(cancel_generations, id);
            CommandStep::CancellableEffect {
                id,
                effect,
                generation: next,
            }
        }
        (step, _) => step,
    }
}

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

    /// Executor used for async futures and streams
    pub(crate) async_executor: Arc<dyn AsyncExecutor>,

    /// Executor used primarily for CPU-bound work
    pub(crate) compute_executor: Option<Arc<dyn BlockingExecutor>>,

    /// Executor used primarily for blocking I/O work
    pub(crate) blocking_executor: Option<Arc<dyn BlockingExecutor>>,

    /// User-provided effect handler
    pub(crate) effect_handler: EffectHandlerFn<E, X, R>,

    /// Shared application resources cloned per effect invocation
    pub(crate) resources: R,

    /// Activity tracker for in-flight async work
    pub(crate) activity: Activity,

    /// Latest generation per cancellation ID.
    pub(crate) cancel_generations: CancelGenerationMap,

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
        let step = prepare_effect_step_for_queue(step, Some(&self.cancel_generations));
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

    fn process_effect(
        &mut self,
        effect: X,
        cancellation: Option<(CancelId, u64)>,
    ) -> Result<(), ShellError> {
        let ctx = EffectContext::new(self.resources.clone());
        let task = (self.effect_handler)(effect, &ctx);
        let cancel_guard = cancellation.map(|(id, generation)| {
            let cancel_generations = Arc::clone(&self.cancel_generations);
            Arc::new(move || !is_cancel_generation_current(&cancel_generations, id, generation))
                as CancelGuard
        });

        drive_task_with_activity(
            &self.async_executor,
            self.compute_executor.as_ref(),
            self.blocking_executor.as_ref(),
            task,
            self.event_tx.clone(),
            self.effect_tx.clone(),
            TaskRouting {
                activity: Some(self.activity.clone()),
                stats: Some(self.stats.clone()),
                cancel_generations: Some(Arc::clone(&self.cancel_generations)),
                cancel_guard,
                panic_handler: self.panic_handler.clone(),
            },
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
                CommandStep::CancellableEffect {
                    id,
                    effect,
                    generation,
                } => {
                    _effect_count += 1;
                    #[cfg(feature = "tracing")]
                    debug!("Routing cancellable effect to Shell");

                    self.push_effect_step(CommandStep::CancellableEffect {
                        id,
                        effect,
                        generation,
                    })?;
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
                self.process_effect(effect, None)?;
                Ok(1)
            }
            CommandStep::CancellableEffect {
                id,
                effect,
                generation,
            } => {
                if !is_cancel_generation_current(&self.cancel_generations, id, generation) {
                    return Ok(0);
                }

                self.process_effect(effect, Some((id, generation)))?;
                Ok(1)
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                let mut handled = 0usize;
                for effect in effects {
                    self.process_effect(effect, None)?;
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
        if self.executors_shutdown {
            return;
        }

        self.async_executor.shutdown();
        if let Some(compute_executor) = self.compute_executor.as_ref() {
            compute_executor.shutdown();
        }
        if let Some(blocking_executor) = self.blocking_executor.as_ref() {
            let same_as_compute = self
                .compute_executor
                .as_ref()
                .is_some_and(|compute| Arc::ptr_eq(compute, blocking_executor));
            if !same_as_compute {
                blocking_executor.shutdown();
            }
        }

        self.executors_shutdown = true;
    }

    /// Wait for all registered executors to finish outstanding work.
    pub fn wait_for_executors(&self) {
        self.async_executor.wait();
        if let Some(compute_executor) = self.compute_executor.as_ref() {
            compute_executor.wait();
        }
        if let Some(blocking_executor) = self.blocking_executor.as_ref() {
            let same_as_compute = self
                .compute_executor
                .as_ref()
                .is_some_and(|compute| Arc::ptr_eq(compute, blocking_executor));
            if !same_as_compute {
                blocking_executor.wait();
            }
        }
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

#[cfg(test)]
mod tests {
    use super::Shell;
    use crate::command::Command;
    use crate::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle, Task};
    use crate::extract::{EffectContext, EventContext};
    use crate::syzygy::{Syzygy, SyzygyConfig};
    use futures_util::future::{BoxFuture, FutureExt};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::thread::JoinHandle;
    use std::time::Duration;

    #[derive(Clone)]
    enum Event {
        LaunchSameId {
            first_gate: std::sync::Arc<AtomicBool>,
            second_gate: std::sync::Arc<AtomicBool>,
        },
        LaunchDifferentIds {
            first_gate: std::sync::Arc<AtomicBool>,
            second_gate: std::sync::Arc<AtomicBool>,
        },
        LaunchNonCancellable {
            first_gate: std::sync::Arc<AtomicBool>,
            second_gate: std::sync::Arc<AtomicBool>,
        },
        Completed(&'static str),
    }

    #[derive(Clone)]
    enum Effect {
        Delayed {
            value: &'static str,
            gate: std::sync::Arc<AtomicBool>,
        },
    }

    struct ThreadAsync {
        shutdown: AtomicBool,
        handles: Mutex<Vec<JoinHandle<()>>>,
    }

    impl ThreadAsync {
        fn new() -> Self {
            Self {
                shutdown: AtomicBool::new(false),
                handles: Mutex::new(Vec::new()),
            }
        }
    }

    impl AsyncExecutor for ThreadAsync {
        fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
            if self.shutdown.load(Ordering::Relaxed) {
                return Err(ExecutorError::Shutdown);
            }

            let handle = std::thread::spawn(move || futures::executor::block_on(job));
            let mut handles = match self.handles.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            handles.push(handle);
            Ok(())
        }

        fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
            async move { std::thread::sleep(duration) }.boxed()
        }
    }

    impl ExecutorLifecycle for ThreadAsync {
        fn shutdown(&self) {
            self.shutdown.store(true, Ordering::Relaxed);
        }

        fn wait(&self) {
            let mut handles = match self.handles.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };

            while let Some(handle) = handles.pop() {
                let _ = handle.join();
            }
        }
    }

    fn handle_event(event: Event, ctx: &EventContext<Vec<&'static str>>) -> Command<Event, Effect> {
        match event {
            Event::LaunchSameId {
                first_gate,
                second_gate,
            } => Command::cancellable(
                "search",
                Effect::Delayed {
                    value: "first",
                    gate: first_gate,
                },
            )
            .and_cancellable(
                "search",
                Effect::Delayed {
                    value: "second",
                    gate: second_gate,
                },
            ),
            Event::LaunchDifferentIds {
                first_gate,
                second_gate,
            } => Command::cancellable(
                "first",
                Effect::Delayed {
                    value: "first",
                    gate: first_gate,
                },
            )
            .and_cancellable(
                "second",
                Effect::Delayed {
                    value: "second",
                    gate: second_gate,
                },
            ),
            Event::LaunchNonCancellable {
                first_gate,
                second_gate,
            } => Command::effect(Effect::Delayed {
                value: "first",
                gate: first_gate,
            })
            .and_effect(Effect::Delayed {
                value: "second",
                gate: second_gate,
            }),
            Event::Completed(value) => {
                // SAFETY: EventContext points to the active model for this dispatch.
                let model = unsafe { &mut *ctx.model_ptr() };
                model.push(value);
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<()>) -> Task<Event, Effect> {
        match effect {
            Effect::Delayed { value, gate } => Task::future(move |_rt| async move {
                while !gate.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }

                Command::event(Event::Completed(value))
            }),
        }
    }

    type TestSyzygy = Syzygy<Event, Effect, Vec<&'static str>, ()>;

    fn build_runner() -> TestSyzygy {
        Syzygy::builder::<Event, Effect>()
            .model(Vec::new())
            .event_handler(handle_event)
            .effect_handler(handle_effect)
            .async_executor(ThreadAsync::new())
            .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(0)))
            .build()
    }

    fn step_n(runner: &mut TestSyzygy, steps: usize) {
        for _ in 0..steps {
            if let Err(err) = runner.step() {
                panic!("step failed: {err}");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn step_until(
        runner: &mut TestSyzygy,
        max_steps: usize,
        predicate: impl Fn(&[&'static str]) -> bool,
    ) {
        for _ in 0..max_steps {
            if predicate(runner.model().as_slice()) {
                return;
            }
            step_n(runner, 1);
        }

        panic!("condition not met within {max_steps} steps");
    }

    #[test]
    fn same_cancel_id_only_latest_result_is_routed() {
        let mut runner = build_runner();
        let first_gate = std::sync::Arc::new(AtomicBool::new(false));
        let second_gate = std::sync::Arc::new(AtomicBool::new(true));

        if let Err(err) = runner.core().try_send_event(Event::LaunchSameId {
            first_gate: std::sync::Arc::clone(&first_gate),
            second_gate,
        }) {
            panic!("failed to queue launch event: {err}");
        }

        step_until(&mut runner, 200, |values| values.contains(&"second"));
        first_gate.store(true, Ordering::SeqCst);
        step_n(&mut runner, 60);

        assert_eq!(runner.model().as_slice(), ["second"]);
    }

    #[test]
    fn different_cancel_ids_run_independently() {
        let mut runner = build_runner();
        let first_gate = std::sync::Arc::new(AtomicBool::new(false));
        let second_gate = std::sync::Arc::new(AtomicBool::new(false));

        if let Err(err) = runner.core().try_send_event(Event::LaunchDifferentIds {
            first_gate: std::sync::Arc::clone(&first_gate),
            second_gate: std::sync::Arc::clone(&second_gate),
        }) {
            panic!("failed to queue launch event: {err}");
        }

        step_n(&mut runner, 5);
        first_gate.store(true, Ordering::SeqCst);
        second_gate.store(true, Ordering::SeqCst);
        step_until(&mut runner, 200, |values| values.len() == 2);

        let mut values = runner.model().clone();
        values.sort_unstable();
        assert_eq!(values, vec!["first", "second"]);
    }

    #[test]
    fn non_cancellable_effects_are_unchanged() {
        let mut runner = build_runner();
        let first_gate = std::sync::Arc::new(AtomicBool::new(false));
        let second_gate = std::sync::Arc::new(AtomicBool::new(true));

        if let Err(err) = runner.core().try_send_event(Event::LaunchNonCancellable {
            first_gate: std::sync::Arc::clone(&first_gate),
            second_gate,
        }) {
            panic!("failed to queue launch event: {err}");
        }

        step_until(&mut runner, 200, |values| values.contains(&"second"));
        first_gate.store(true, Ordering::SeqCst);
        step_until(&mut runner, 200, |values| values.len() == 2);

        let mut values = runner.model().clone();
        values.sort_unstable();
        assert_eq!(values, vec!["first", "second"]);
    }

    #[test]
    fn shell_queue_assigns_generation_to_cancellable_effects() {
        let runner = build_runner();
        let mut shell: Shell<Event, Effect> = runner.split().1;

        shell
            .dispatch_command(Command::cancellable(
                "search",
                Effect::Delayed {
                    value: "first",
                    gate: std::sync::Arc::new(AtomicBool::new(true)),
                },
            ))
            .unwrap_or_else(|err| panic!("dispatch failed: {err}"));

        shell
            .dispatch_command(Command::cancellable(
                "search",
                Effect::Delayed {
                    value: "second",
                    gate: std::sync::Arc::new(AtomicBool::new(true)),
                },
            ))
            .unwrap_or_else(|err| panic!("dispatch failed: {err}"));

        let first = shell
            .next_effect_step()
            .unwrap_or_else(|| panic!("missing first step"));
        let second = shell
            .next_effect_step()
            .unwrap_or_else(|| panic!("missing second step"));

        match first {
            crate::command::CommandStep::CancellableEffect { generation, .. } => {
                assert_eq!(generation, 1);
            }
            _ => panic!("unexpected first step variant"),
        }

        match second {
            crate::command::CommandStep::CancellableEffect { generation, .. } => {
                assert_eq!(generation, 2);
            }
            _ => panic!("unexpected second step variant"),
        }
    }
}
