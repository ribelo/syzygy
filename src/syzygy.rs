use std::thread;
use std::time::Duration;

use crate::core::Core;
use crate::error::ShellError;
use crate::shell::Shell;

#[derive(Clone, Debug)]
pub struct SyzygyConfig {
    pub idle_sleep: Duration,
}

impl Default for SyzygyConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(1),
        }
    }
}

impl SyzygyConfig {
    #[must_use]
    pub fn idle_sleep(mut self, duration: impl Into<Duration>) -> Self {
        self.idle_sleep = duration.into();
        self
    }
}

pub struct Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
{
    core: Core<Event, Effect, Model>,
    shell: Shell<Event, Effect>,
    config: SyzygyConfig,
}

pub type Runner<Event, Effect, Model> = Syzygy<Event, Effect, Model>;

impl<Event, Effect, Model> Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
{
    pub fn new(core: Core<Event, Effect, Model>, shell: Shell<Event, Effect>) -> Self {
        Self {
            core,
            shell,
            config: SyzygyConfig::default(),
        }
    }

    pub fn with_config(
        core: Core<Event, Effect, Model>,
        shell: Shell<Event, Effect>,
        config: SyzygyConfig,
    ) -> Self {
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

                park_for_runtime(self.config.idle_sleep);
            }
        }
    }

    pub fn run_until<F>(&mut self, mut condition: F) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect>) -> bool,
    {
        while !condition(&self.core, &self.shell) {
            let did_work = self.step()?;
            if self.shell.is_closed() {
                return Ok(());
            }

            if !did_work {
                park_for_runtime(self.config.idle_sleep);
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

    pub fn shell(&self) -> &Shell<Event, Effect> {
        &self.shell
    }

    pub fn shell_mut(&mut self) -> &mut Shell<Event, Effect> {
        &mut self.shell
    }

    pub fn config(&self) -> &SyzygyConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: SyzygyConfig) {
        self.config = config;
    }

    pub fn shutdown(&mut self) {
        self.shell.shutdown();
        self.shell.wait_for_executors();
    }

    pub fn step(&mut self) -> Result<bool, ShellError> {
        step_core_shell(&mut self.core, &mut self.shell)
    }

    pub fn split(self) -> (Core<Event, Effect, Model>, Shell<Event, Effect>) {
        (self.core, self.shell)
    }
}

pub fn step_core_shell<Event, Effect, Model>(
    core: &mut Core<Event, Effect, Model>,
    shell: &mut Shell<Event, Effect>,
) -> Result<bool, ShellError>
where
    Event: 'static,
    Effect: 'static,
{
    let mut core_work = false;
    let mut dispatch_error = None;

    core.process_events_into(|command| {
        core_work = true;
        if dispatch_error.is_none() {
            if let Err(error) = shell.dispatch_command(command) {
                dispatch_error = Some(error);
            }
        }
    });

    if let Some(error) = dispatch_error {
        return Err(error);
    }

    let shell_work = shell.drain()?;
    Ok(core_work || shell_work > 0)
}

fn park_for_runtime(idle_sleep: Duration) {
    if compio::runtime::Runtime::try_with_current(|runtime| runtime.poll_with(Some(idle_sleep)))
        .is_ok()
    {
        return;
    }

    if idle_sleep.is_zero() {
        thread::yield_now();
    } else {
        thread::sleep(idle_sleep);
    }
}

impl<Event, Effect, Model> From<(Core<Event, Effect, Model>, Shell<Event, Effect>)>
    for Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
{
    fn from(parts: (Core<Event, Effect, Model>, Shell<Event, Effect>)) -> Self {
        Self::new(parts.0, parts.1)
    }
}

impl<Event, Effect, Model> std::fmt::Debug for Syzygy<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
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
