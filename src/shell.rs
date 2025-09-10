//! # Shell - Asynchronous Effect Management
//!
//! The Shell orchestrates asynchronous effect execution, bridging pure Core updates
//! with side-effectful operations. Effect handlers return `EffectSpec` plans which
//! the Shell drives using executors registered in an immutable registry.

use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use crate::command::executor::route_command;
use crate::command::{Command, CommandStep};
use crate::effect_context::EffectContext;
use crate::error::ShellError;
use crate::executor::ExecutorRegistry;
use crate::executor::spec::drive_spec;

use crate::timer::{Time, time};

use crate::effect_handler::EffectHandler;
// use futures_util::future::*;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span, warn};

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
    E: Clone + Send + Sync + 'static,
    X: Clone + Send + 'static,
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
    E: Clone + Send + Sync + 'static,
    X: Clone + Send + 'static,
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
    /// FCIS note: This is crate-visible to prevent external code from
    /// dispatching effects directly. Only Core (via Runner) should call this.
    pub(crate) fn dispatch(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        #[cfg(feature = "tracing")]
        debug!("Executing command");

        let effect_sender = &self.effect_tx;
        let event_sender = &self.event_tx;

        if let Err(e) = route_command(command, event_sender, effect_sender) {
            #[cfg(feature = "tracing")]
            warn!("Command execution failed: {:?}", e);
            eprintln!("Command execution failed: {e:?}");
            return Err(ShellError::CommandExecutionFailed(e.to_string()));
        }

        Ok(())
    }

    /// Process effects and manage async tasks
    #[allow(clippy::unused_async)]
    pub async fn tick<S>(&mut self, spawner: S) -> Result<bool, ShellError>
    where
        S: crate::spawn::Spawn,
    {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "shell_tick").entered();

        let mut did_work = false;
        #[allow(unused_variables)]
        let mut effect_count = 0usize;

        while let Ok(step) = self.effect_rx.try_recv() {
            match step {
                CommandStep::Effect(fx) => {
                    did_work = true;
                    effect_count += 1;
                    let ctx = EffectContext::new(
                        self.event_tx.clone(),
                        self.resources.clone(),
                        Arc::clone(&self.executors),
                    );
                    let spec = (self.effect_handler)(fx, &ctx);
                    spawner.spawn(drive_spec(spec, ctx));
                }
                CommandStep::Batch(effects) => {
                    did_work = true;
                    effect_count += effects.len();
                    let ctx = EffectContext::new(
                        self.event_tx.clone(),
                        self.resources.clone(),
                        Arc::clone(&self.executors),
                    );
                    let handler = self.effect_handler;
                    spawner.spawn(async move {
                        for fx in effects {
                            let spec = handler(fx, &ctx);
                            drive_spec(spec, ctx.clone()).await;
                        }
                    });
                }
                CommandStep::Event(_) => unreachable!(),
            }
        }

        #[cfg(feature = "tracing")]
        if did_work {
            debug!(effects = effect_count, "Shell tick completed");
        }

        Ok(did_work)
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
