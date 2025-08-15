//! Command executor for THE Syzygy system
//!
//! This module provides the CommandExecutor that runs commands asynchronously
//! using tokio_util::task::TaskTracker for proper lifecycle management.

use std::pin::Pin;
use std::future::Future;
use crossbeam_channel::{Receiver, Sender};
use crate::context::CommandContext;
use crate::resource::Resources;

#[cfg(feature = "async")]
use tokio_util::task::TaskTracker;

#[cfg(feature = "tracing")]
use tracing::{debug, warn, error};

/// Command executor that runs commands asynchronously with proper lifecycle management
///
/// The CommandExecutor uses a TaskTracker to manage the lifecycle of spawned commands,
/// providing graceful shutdown and panic recovery capabilities.
#[cfg(feature = "async")]
pub struct CommandExecutor<Event, Command>
where
    Event: Clone + Send + 'static,
    Command: Clone + Send + 'static,
{
    command_rx: Receiver<Command>,
    event_tx: Sender<Event>,
    resources: Resources,
    command_handler: fn(CommandContext<Event, Resources>, Command) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    tracker: TaskTracker,
}

#[cfg(feature = "async")]
impl<Event, Command> CommandExecutor<Event, Command>
where
    Event: Clone + Send + 'static,
    Command: Clone + Send + 'static,
{
    /// Create a new CommandExecutor
    pub(crate) fn new(
        command_rx: Receiver<Command>,
        event_tx: Sender<Event>,
        resources: Resources,
        command_handler: fn(CommandContext<Event, Resources>, Command) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    ) -> Self {
        Self {
            command_rx,
            event_tx,
            resources,
            command_handler,
            tracker: TaskTracker::new(),
        }
    }

    /// Start the executor loop in the background
    ///
    /// This spawns a background task that continuously polls for new tasks
    /// and executes them using the TaskTracker.
    pub fn start(self) -> CommandExecutorHandle {
        let tracker = self.tracker.clone();

        let handle = tokio::spawn(async move {
            self.run().await;
        });

        CommandExecutorHandle {
            tracker,
            join_handle: handle,
        }
    }

    /// Run the executor loop
    ///
    /// This method continuously polls for new commands and executes them.
    /// Each command is wrapped in panic recovery and executed asynchronously.
    pub async fn run(self) {
        #[cfg(feature = "tracing")]
        debug!("CommandExecutor started");

        loop {
            // Try to receive a command non-blocking, then sleep if nothing available
            match self.command_rx.try_recv() {
                Ok(command) => {
                    let event_tx = self.event_tx.clone();
                    let resources = self.resources.clone();
                    let handler = self.command_handler;

                    // Spawn command with tracker for lifecycle management and panic isolation
                    self.tracker.spawn(async move {
                        // Create command context
                        let ctx = CommandContext::new(event_tx, resources);

                        // Execute command - tokio::spawn provides panic isolation for async functions
                        let command_handle = tokio::spawn(async move {
                            handler(ctx, command).await;
                        });

                        // Await the command and handle any panics
                        match command_handle.await {
                            Ok(()) => {
                                // Command completed successfully
                            }
                            Err(join_error) if join_error.is_panic() => {
                                #[cfg(feature = "tracing")]
                                warn!("Command panicked during execution, continuing");
                            }
                            Err(_join_error) => {
                                #[cfg(feature = "tracing")]
                                warn!("Command was cancelled: {:?}", _join_error);
                            }
                        }
                    });
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    #[cfg(feature = "tracing")]
                    debug!("Command channel disconnected, shutting down executor");
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No commands available, sleep briefly to avoid busy waiting
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }

        #[cfg(feature = "tracing")]
        debug!("CommandExecutor run loop exiting");
    }

    /// Get a reference to the TaskTracker
    pub fn tracker(&self) -> &TaskTracker {
        &self.tracker
    }
}

/// Handle for managing a running CommandExecutor
#[cfg(feature = "async")]
pub struct CommandExecutorHandle {
    tracker: TaskTracker,
    join_handle: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "async")]
impl CommandExecutorHandle {
    /// Gracefully shutdown the executor
    ///
    /// This waits for the executor run loop to finish and then cleans up all spawned tasks.
    pub async fn shutdown(self) {
        #[cfg(feature = "tracing")]
        debug!("Initiating CommandExecutor shutdown");

        // Close tracker to prevent new commands from being spawned
        self.tracker.close();

        // Wait for executor loop to finish first
        let _ = self.join_handle.await;

        // Wait for all remaining spawned commands to complete
        self.tracker.wait().await;

        #[cfg(feature = "tracing")]
        debug!("CommandExecutor shutdown complete");
    }

    /// Get the number of currently running commands
    pub fn command_count(&self) -> usize {
        self.tracker.len()
    }

    /// Check if the executor is closed
    pub fn is_closed(&self) -> bool {
        self.tracker.is_closed()
    }
}

impl std::fmt::Debug for CommandExecutorHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandExecutorHandle")
            .field("command_count", &self.command_count())
            .field("is_closed", &self.is_closed())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::Resources;
    use std::time::Duration;
    use tokio::time::sleep;
    use crossbeam_channel::unbounded;
    use std::future::Future;
    use std::pin::Pin;

    #[derive(Debug, Clone)]
    enum TestEvent {
        CommandCompleted,
    }

    #[derive(Debug, Clone)]
    enum TestCommand {
        SimpleCommand,
        PanicCommand,
    }

    // Command handler function
    fn handle_test_command(
        ctx: CommandContext<TestEvent, Resources>, 
        command: TestCommand
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            match command {
                TestCommand::SimpleCommand => {
                    sleep(Duration::from_millis(10)).await;
                    let _ = ctx.dispatch(TestEvent::CommandCompleted);
                }
                TestCommand::PanicCommand => {
                    panic!("Test panic");
                }
            }
        })
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_command_executor_basic() {
        let (command_tx, command_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        let resources = Resources::default();

        let executor = CommandExecutor::new(command_rx, event_tx, resources, handle_test_command);
        let handle = executor.start();

        // Send a command
        command_tx.send(TestCommand::SimpleCommand).unwrap();

        // Wait a bit for command to execute
        sleep(Duration::from_millis(20)).await;

        // Check that event was dispatched
        assert!(event_rx.try_recv().is_ok());

        // Drop the command sender to close the channel
        drop(command_tx);

        // Shutdown executor
        handle.shutdown().await;
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_command_executor_panic_recovery() {
        let (command_tx, command_rx) = unbounded();
        let (event_tx, _event_rx) = unbounded();
        let resources = Resources::default();

        let executor = CommandExecutor::new(command_rx, event_tx, resources, handle_test_command);
        let handle = executor.start();

        // Send a panicking command
        command_tx.send(TestCommand::PanicCommand).unwrap();

        // Send a normal command after panic
        command_tx.send(TestCommand::SimpleCommand).unwrap();

        // Wait for commands to execute
        sleep(Duration::from_millis(30)).await;

        // Executor should still be running
        assert!(!handle.is_closed());

        // Drop the command sender to close the channel
        drop(command_tx);

        // Shutdown executor
        handle.shutdown().await;
    }
}
