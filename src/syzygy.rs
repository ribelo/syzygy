use std::time::Duration;

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
pub struct DiagnosticsConfig {
    pub unhandled_effects: UnhandledEffectPolicy,
    pub trace: ShellTraceConfig,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            unhandled_effects: UnhandledEffectPolicy::Error,
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
}

pub struct Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
{
    core: Core<Event, Effect, Model>,
    shell: Shell<Event, Effect, Model>,
    config: SyzygyConfig,
}

pub type Runner<Event, Effect, Model> = Syzygy<Event, Effect, Model>;

impl<Event, Effect, Model> Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
{
    pub fn new(core: Core<Event, Effect, Model>, shell: Shell<Event, Effect, Model>) -> Self {
        Self::with_config(core, shell, SyzygyConfig::default())
    }

    pub fn with_config(
        core: Core<Event, Effect, Model>,
        mut shell: Shell<Event, Effect, Model>,
        config: SyzygyConfig,
    ) -> Self {
        shell.set_unhandled_effects_policy(config.diagnostics.unhandled_effects);
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
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Model>) -> bool,
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

    #[must_use]
    pub fn model(&self) -> &Model {
        self.core.model()
    }

    pub fn model_mut(&mut self) -> &mut Model {
        self.core.model_mut()
    }

    pub fn core(&self) -> &Core<Event, Effect, Model> {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut Core<Event, Effect, Model> {
        &mut self.core
    }

    pub fn shell(&self) -> &Shell<Event, Effect, Model> {
        &self.shell
    }

    pub fn shell_mut(&mut self) -> &mut Shell<Event, Effect, Model> {
        &mut self.shell
    }

    pub fn config(&self) -> &SyzygyConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SyzygyConfig) {
        self.shell
            .set_unhandled_effects_policy(config.diagnostics.unhandled_effects);
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

    pub fn split(self) -> (Core<Event, Effect, Model>, Shell<Event, Effect, Model>) {
        (self.core, self.shell)
    }
}

pub fn step_core_shell<Event, Effect, Model>(
    core: &mut Core<Event, Effect, Model>,
    shell: &mut Shell<Event, Effect, Model>,
) -> Result<bool, ShellError>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
{
    if shell.is_closed() {
        return Ok(false);
    }

    let mut boot_work = 0usize;
    if let Some(boot_command) = shell.take_boot_command_for_model(core.model()) {
        shell.record_trace_event(ShellTraceEvent::BootCommandDispatched);
        let mut non_event_steps = Vec::new();
        for step in boot_command {
            match step {
                CommandStep::Event(event) => {
                    core.enqueue_event(event);
                    boot_work = 1;
                }
                other => non_event_steps.push(other),
            }
        }

        if !non_event_steps.is_empty() {
            shell.dispatch_command(non_event_steps.into_iter().collect::<Command<_, _>>())?;
            boot_work = 1;
        }
    }
    let core_work = core.process_events_try_into(|command| {
        shell.record_trace_event(ShellTraceEvent::CoreEventCommandDispatched);
        shell.dispatch_command(command)
    })?;
    shell.record_trace_event(ShellTraceEvent::CoreEventsProcessed { count: core_work });
    let subscription_work = shell.reconcile_subscriptions_for_model(core.model())?;
    let shell_work = shell.drain()?;
    Ok(boot_work > 0 || core_work > 0 || subscription_work > 0 || shell_work > 0)
}

impl<Event, Effect, Model> From<(Core<Event, Effect, Model>, Shell<Event, Effect, Model>)>
    for Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
{
    fn from(parts: (Core<Event, Effect, Model>, Shell<Event, Effect, Model>)) -> Self {
        Self::new(parts.0, parts.1)
    }
}

impl<Event, Effect, Model> std::fmt::Debug for Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
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
