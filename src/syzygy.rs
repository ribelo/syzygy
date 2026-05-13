use std::time::{Duration, Instant};

use crate::command::{Command, CommandStep};
use crate::core::Core;
use crate::error::ShellError;
use crate::shell::{Shell, ShellTraceConfig, ShellTraceEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnhandledEffectPolicy {
    Error,
    Ignore,
}

impl Default for UnhandledEffectPolicy {
    fn default() -> Self {
        Self::Error
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeferredEventOverflowPolicy {
    Error,
    DropNewest,
}

impl Default for DeferredEventOverflowPolicy {
    fn default() -> Self {
        Self::Error
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticsConfig {
    pub unhandled_effects: UnhandledEffectPolicy,
    pub deferred_event_overflow: DeferredEventOverflowPolicy,
    pub trace: ShellTraceConfig,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            unhandled_effects: UnhandledEffectPolicy::Error,
            deferred_event_overflow: DeferredEventOverflowPolicy::Error,
            trace: ShellTraceConfig::default(),
        }
    }
}

impl DiagnosticsConfig {
    #[must_use]
    pub fn unhandled_effects(mut self, policy: UnhandledEffectPolicy) -> Self {
        self.unhandled_effects = policy;
        self
    }

    #[must_use]
    pub fn deferred_event_overflow(mut self, policy: DeferredEventOverflowPolicy) -> Self {
        self.deferred_event_overflow = policy;
        self
    }

    #[must_use]
    pub fn trace(mut self, trace: ShellTraceConfig) -> Self {
        self.trace = trace;
        self
    }
}

#[derive(Clone, Debug)]
pub struct SyzygyConfig {
    pub idle_sleep: Duration,
    pub diagnostics: DiagnosticsConfig,
}

impl Default for SyzygyConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(1),
            diagnostics: DiagnosticsConfig::default(),
        }
    }
}

impl SyzygyConfig {
    #[must_use]
    pub fn idle_sleep(mut self, duration: impl Into<Duration>) -> Self {
        self.idle_sleep = duration.into();
        self
    }

    #[must_use]
    pub fn diagnostics(mut self, diagnostics: DiagnosticsConfig) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    #[must_use]
    pub fn unhandled_effects(mut self, policy: UnhandledEffectPolicy) -> Self {
        self.diagnostics = self.diagnostics.unhandled_effects(policy);
        self
    }

    #[must_use]
    pub fn deferred_event_overflow(mut self, policy: DeferredEventOverflowPolicy) -> Self {
        self.diagnostics = self.diagnostics.deferred_event_overflow(policy);
        self
    }
}

pub struct Syzygy<Event, Effect, Model, Resources = ()>
where
    Event: 'static,
    Effect: 'static,
    Resources: 'static,
{
    core: Core<Event, Effect, Model>,
    shell: Shell<Event, Effect, Model, Resources>,
    config: SyzygyConfig,
}

pub type Runner<Event, Effect, Model, Resources = ()> = Syzygy<Event, Effect, Model, Resources>;
pub type SyzygyParts<Event, Effect, Model, Resources = ()> = (
    Core<Event, Effect, Model>,
    Shell<Event, Effect, Model, Resources>,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunUntilExit {
    ConditionMet,
    Cancelled,
    ShellClosed,
}

impl<Event, Effect, Model, Resources> Syzygy<Event, Effect, Model, Resources>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
    Resources: 'static,
{
    pub fn new(
        core: Core<Event, Effect, Model>,
        shell: Shell<Event, Effect, Model, Resources>,
    ) -> Self {
        Self::with_config(core, shell, SyzygyConfig::default())
    }

    pub fn with_config(
        core: Core<Event, Effect, Model>,
        mut shell: Shell<Event, Effect, Model, Resources>,
        config: SyzygyConfig,
    ) -> Self {
        shell.set_unhandled_effects_policy(config.diagnostics.unhandled_effects);
        shell.set_deferred_event_overflow_policy(config.diagnostics.deferred_event_overflow);
        shell.set_trace_config(config.diagnostics.trace);
        Self {
            core,
            shell,
            config,
        }
    }

    pub fn run(&mut self) -> Result<(), ShellError> {
        loop {
            let did_work = self.step()?;
            if self.shell.is_closed() {
                return Ok(());
            }

            if !did_work {
                if self.shell.is_idle() {
                    return Ok(());
                }

                self.shell.park_runtime(self.config.idle_sleep);
            }
        }
    }

    pub fn run_until<F>(&mut self, mut condition: F) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Model, Resources>) -> bool,
    {
        while self.shell.has_pending_boot() || !condition(&self.core, &self.shell) {
            let did_work = self.step()?;
            if self.shell.is_closed() {
                return Ok(());
            }

            if !did_work {
                self.shell.park_runtime(self.config.idle_sleep);
            }
        }

        Ok(())
    }

    pub fn run_until_timeout<F>(
        &mut self,
        timeout: Duration,
        condition: F,
    ) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Model, Resources>) -> bool,
    {
        let start = Instant::now();
        let Some(deadline) = start.checked_add(timeout) else {
            return self.run_until(condition);
        };

        self.run_until_deadline(deadline, condition)
            .map_err(|error| match error {
                ShellError::Timeout { .. } => ShellError::Timeout { duration: timeout },
                other => other,
            })
    }

    pub fn run_until_deadline<F>(
        &mut self,
        deadline: Instant,
        mut condition: F,
    ) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Model, Resources>) -> bool,
    {
        let start = Instant::now();
        while self.shell.has_pending_boot() || !condition(&self.core, &self.shell) {
            Self::fail_if_deadline_elapsed(start, deadline)?;
            let did_work = self.step()?;
            if self.shell.is_closed() {
                return Ok(());
            }

            if !did_work {
                self.park_until_deadline(start, deadline)?;
            }
        }

        Ok(())
    }

    pub fn run_until_or_cancelled<F, C>(
        &mut self,
        mut condition: F,
        mut should_cancel: C,
    ) -> Result<RunUntilExit, ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Model, Resources>) -> bool,
        C: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Model, Resources>) -> bool,
    {
        while self.shell.has_pending_boot() || !condition(&self.core, &self.shell) {
            if should_cancel(&self.core, &self.shell) {
                return Ok(RunUntilExit::Cancelled);
            }

            let did_work = self.step()?;
            if self.shell.is_closed() {
                return Ok(RunUntilExit::ShellClosed);
            }

            if !did_work {
                self.shell.park_runtime(self.config.idle_sleep);
            }
        }

        Ok(RunUntilExit::ConditionMet)
    }

    #[must_use]
    pub fn model(&self) -> &Model {
        self.core.model()
    }

    /// Advanced escape hatch.
    ///
    /// Prefer driving state transitions through events (`core().try_send(...)` + `step`/`run`)
    /// in normal application code. Direct mutable access can bypass handler invariants and
    /// make traces/snapshots harder to interpret.
    ///
    /// Valid use cases: focused tests, migration code, or controlled integration shims.
    pub fn model_mut(&mut self) -> &mut Model {
        self.core.model_mut()
    }

    pub fn core(&self) -> &Core<Event, Effect, Model> {
        &self.core
    }

    /// Advanced escape hatch.
    ///
    /// Mutating `Core` directly can bypass the usual event-processing boundary. Callers must
    /// preserve FIFO/event ordering assumptions and avoid mixing direct mutations with in-flight
    /// shell work in ways that violate app expectations.
    pub fn core_mut(&mut self) -> &mut Core<Event, Effect, Model> {
        &mut self.core
    }

    pub fn shell(&self) -> &Shell<Event, Effect, Model, Resources> {
        &self.shell
    }

    /// Advanced escape hatch.
    ///
    /// Direct shell mutation can change runtime/diagnostic state outside the `step` loop.
    /// Prefer `run`, `run_until`, `snapshot`, and trace APIs for normal operation.
    ///
    /// Valid use cases: runtime integration boundaries and specialized tests.
    pub fn shell_mut(&mut self) -> &mut Shell<Event, Effect, Model, Resources> {
        &mut self.shell
    }

    pub fn config(&self) -> &SyzygyConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SyzygyConfig) {
        self.shell
            .set_unhandled_effects_policy(config.diagnostics.unhandled_effects);
        self.shell
            .set_deferred_event_overflow_policy(config.diagnostics.deferred_event_overflow);
        self.shell.set_trace_config(config.diagnostics.trace);
        self.config = config;
    }

    pub fn shutdown(&mut self) {
        self.shell.shutdown();
        self.shell.wait_for_executors();
    }

    pub fn step(&mut self) -> Result<bool, ShellError> {
        step_core_shell(&mut self.core, &mut self.shell)
    }

    fn fail_if_deadline_elapsed(start: Instant, deadline: Instant) -> Result<(), ShellError> {
        let now = Instant::now();
        if now >= deadline {
            return Err(ShellError::Timeout {
                duration: now.saturating_duration_since(start),
            });
        }

        Ok(())
    }

    fn park_until_deadline(&self, start: Instant, deadline: Instant) -> Result<(), ShellError> {
        let now = Instant::now();
        if now >= deadline {
            return Err(ShellError::Timeout {
                duration: now.saturating_duration_since(start),
            });
        }

        let remaining = deadline.saturating_duration_since(now);
        let park_duration = self.config.idle_sleep.min(remaining);
        self.shell.park_runtime(park_duration);
        Self::fail_if_deadline_elapsed(start, deadline)
    }

    /// Advanced escape hatch.
    ///
    /// Splitting transfers orchestration responsibility to the caller. If you drive `Core` and
    /// `Shell` separately, preserve the usual progression (`process events` -> `dispatch` ->
    /// `reconcile subscriptions` -> `drain shell`) to avoid semantic drift.
    pub fn split(self) -> SyzygyParts<Event, Effect, Model, Resources> {
        (self.core, self.shell)
    }
}

pub fn step_core_shell<Event, Effect, Model, Resources>(
    core: &mut Core<Event, Effect, Model>,
    shell: &mut Shell<Event, Effect, Model, Resources>,
) -> Result<bool, ShellError>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
    Resources: 'static,
{
    if shell.is_closed() {
        return Ok(false);
    }

    let mut boot_work = 0usize;
    let mut deferred_boot_command = None;
    if let Some(boot_command) = shell.take_boot_command_for_model(core.model()) {
        shell.record_trace_event(ShellTraceEvent::BootCommandDispatched);
        let mut non_event_steps = Vec::new();
        for step in boot_command {
            match step {
                CommandStep::Event(event) => {
                    core.enqueue_event(event);
                }
                other => non_event_steps.push(other),
            }
        }

        if !non_event_steps.is_empty() {
            deferred_boot_command = Some(non_event_steps.into_iter().collect::<Command<_, _>>());
        }

        boot_work = 1;
    }

    let core_work = core.process_events_try_into(|command| {
        shell.record_trace_event(ShellTraceEvent::CoreEventCommandDispatched);
        shell.dispatch_command(command)
    })?;

    if let Some(command) = deferred_boot_command {
        shell.dispatch_command(command)?;
    }

    shell.record_trace_event(ShellTraceEvent::CoreEventsProcessed { count: core_work });
    let subscription_work = shell.reconcile_subscriptions_for_model(core.model())?;
    let shell_work = shell.drain()?;
    Ok(boot_work > 0 || core_work > 0 || subscription_work > 0 || shell_work > 0)
}

impl<Event, Effect, Model, Resources>
    From<(
        Core<Event, Effect, Model>,
        Shell<Event, Effect, Model, Resources>,
    )> for Syzygy<Event, Effect, Model, Resources>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
    Resources: 'static,
{
    /// Advanced reconstruction helper for integration/test code.
    ///
    /// The caller is responsible for passing matching `Core`/`Shell` parts that uphold Syzygy's
    /// boundary assumptions.
    fn from(
        parts: (
            Core<Event, Effect, Model>,
            Shell<Event, Effect, Model, Resources>,
        ),
    ) -> Self {
        Self::new(parts.0, parts.1)
    }
}

impl<Event, Effect, Model, Resources> std::fmt::Debug for Syzygy<Event, Effect, Model, Resources>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
    Resources: 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syzygy")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Syzygy<(), (), ()> {
    #[must_use]
    pub fn builder<NewEvent, NewEffect>() -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, ()>
    where
        NewEvent: 'static,
        NewEffect: 'static,
    {
        crate::builder::SyzygyBuilder::new()
    }
}
