//! # Shell - Asynchronous Effect Management
//!
//! This module provides the `Shell` component, which is responsible for managing
//! asynchronous side effects. It works in tandem with the `Core` to separate
//! synchronous state updates from asynchronous operations.
//!
//! ## Key Components
//! - `Shell` - The main struct that orchestrates effect execution.
//! - `EffectHandler` - A trait for implementing effect handling logic.
//! - `EffectContext` - Provides services to effect handlers, like sending events.
//!
//! ## Example
//! ```rust
//! # use syzygy::prelude::*;
//! # use syzygy::event_context::EventContext;
//! # use syzygy::storage::{EmptyStorage, Storage};
//! # #[derive(Debug, Clone)] enum TestEvent { Ping }
//! # #[derive(Debug, Clone)] enum TestEffect { DoPing }
//! # #[derive(Debug, Default)] struct Model;
//! # fn update(event: TestEvent, ctx: &mut EventContext<TestEvent, TestEffect, Storage<Model, EmptyStorage>>) -> Command<TestEvent, TestEffect> {
//! #     Command::effect(TestEffect::DoPing)
//! # }
//! # async fn handle_effects(effect: TestEffect, ctx: EffectContext<TestEvent, EmptyStorage>) {
//! #     if let TestEffect::DoPing = effect {
//! #         println!("Pong!");
//! #     }
//! # }
//! // In a real application, you would build the shell like this:
//! let (core, shell) = Syzygy::builder()
//!     .model(Model::default())
//!     .event_handler(update)
//!     .effect_handler(handle_effects)
//!     .build();
//!
//! // The shell would then be used by a Runner to execute effects.
//! ```
use crossbeam_channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::async_context::EffectContext;
use crate::command::executor::route_command;
use crate::command::{Command, CommandStep, GroupMode};
use crate::error::ShellError;
use crate::task::{TaskStats, TaskTracker};
use crate::timer::{Time, time};
use crate::executor::EmptyExecutorStorage;

use crate::effect_handler::EffectHandler;
use futures_concurrency::prelude::*;
use futures::FutureExt;

#[cfg(feature = "tracing")]
use tracing::{Level, debug, span, warn};

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
/// - Providing Resources to effect handlers via Storage
/// - Managing specialized executors for different effect types
///
/// The Shell works alongside Core rather than owning it, giving users
/// maximum flexibility in how they structure their applications.
/// Resources are stored using the same Storage pattern as Core's models.
/// Executors are also stored using the same Storage pattern for type-safe access.
pub struct Shell<Event, Effect, Resources = crate::storage::EmptyStorage, Executors = EmptyExecutorStorage, H = ()>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
    H: EffectHandler<Event, Effect, Resources, Executors> + 'static,
{
    /// Task tracker for managing async effects (wrapped for sharing with EffectContext)
    pub(crate) task_tracker: Arc<Mutex<TaskTracker>>,

    /// Channel for receiving command outputs (effects)
    pub(crate) effect_rx: Receiver<CommandStep<Event, Effect>>,
    pub(crate) effect_tx: Sender<CommandStep<Event, Effect>>,

    /// Channel for sending events back to Core
    pub(crate) event_tx: Option<Sender<Event>>,

    /// Resources shared with effect handlers (user controls Arc wrapping)
    pub(crate) resources: Resources,

    /// Specialized executors for different effect handling strategies
    pub(crate) executors: Executors,

    /// User-provided effect handler (AFIT, zero-alloc). Default is `()` which is a noop handler.
    pub(crate) effect_handler: H,

    /// Configuration for error handling and timeouts
    pub(crate) config: ShellConfig,
}

impl<Event, Effect> Default for Shell<Event, Effect, crate::storage::EmptyStorage, EmptyExecutorStorage, ()>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Event, Effect> Shell<Event, Effect, crate::storage::EmptyStorage, EmptyExecutorStorage, ()>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
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
            resources: crate::storage::EmptyStorage::new(),
            executors: EmptyExecutorStorage::default(),
            effect_handler: (),
            config,
        }
    }
}

impl<Event, Effect, Resources, Executors, H> Shell<Event, Effect, Resources, Executors, H>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
    Executors: Clone + Send + Sync + 'static,
    H: EffectHandler<Event, Effect, Resources, Executors> + 'static + Clone,
{
    /// Create a new Shell with custom configuration and explicit handler
    #[must_use]
    pub fn with_config_and_handler<H2>(config: ShellConfig, handler: H2) -> Shell<Event, Effect, Resources, Executors, H2>
    where
        Resources: Default,
        Executors: Default,
        H2: EffectHandler<Event, Effect, Resources, Executors> + 'static,
    {
        // Create channel based on configuration
        let (effect_tx, effect_rx) = match config.effect_channel_capacity {
            Some(capacity) => crossbeam_channel::bounded(capacity),
            None => crossbeam_channel::unbounded(),
        };

        Shell {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            effect_rx,
            effect_tx,
            event_tx: None,
            resources: Resources::default(),
            executors: Executors::default(),
            effect_handler: handler,
            config,
        }
    }

    /// Replace the effect handler with a new AFIT handler
    #[must_use]
    pub fn with_effect_handler<H2>(self, handler: H2) -> Shell<Event, Effect, Resources, Executors, H2>
    where
        H2: EffectHandler<Event, Effect, Resources, Executors> + 'static,
    {
        Shell {
            effect_handler: handler,
            task_tracker: self.task_tracker,
            effect_rx: self.effect_rx,
            effect_tx: self.effect_tx,
            event_tx: self.event_tx,
            resources: self.resources,
            executors: self.executors,
            config: self.config,
        }
    }



    /// Set the resources storage for effect handlers
    ///
    /// This replaces the entire resource storage. For adding individual resources,
    /// use the builder pattern with .resource() method.
    #[must_use]
    pub fn with_resources(mut self, resources: Resources) -> Self {
        self.resources = resources;
        self
    }

    /// Get an immutable reference to a specific resource by type
    ///
    /// This provides direct access to resources stored in the Shell.
    /// Resources are always immutable through this API - if you need
    /// mutability, the resource itself should provide interior mutability
    /// (e.g., using Arc<Mutex<>>, RwLock, RefCell, atomic types, etc.).
    ///
    /// # Example
    /// ```rust,ignore
    /// // For read-only resources (most common case)
    /// let http_client: &HttpClient = shell.resource();
    ///
    /// // For resources needing mutability
    /// struct Cache {
    ///     data: Arc<Mutex<HashMap<String, String>>>,
    /// }
    /// let cache: &Cache = shell.resource();
    /// // Use cache.data.lock() when you need to mutate
    /// ```
    #[must_use]
    pub fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>,
    {
        // Resources are Arc-wrapped, so we deref to get to the Storage
        self.resources.get()
    }

    /// Get an immutable reference to a specific executor by type
    ///
    /// This provides direct access to executors stored in the Shell.
    /// Executors can be accessed by their NewType wrapper to distinguish
    /// multiple executors of the same base type.
    ///
    /// # Example
    /// ```rust,ignore
    /// // Using NewType pattern for multiple executors
    /// struct DatabaseExecutor(TokioExecutor);
    /// struct NetworkExecutor(TokioExecutor);
    ///
    /// let db_executor: &DatabaseExecutor = shell.executor();
    /// let net_executor: &NetworkExecutor = shell.executor();
    /// ```
    #[must_use]
    pub fn executor<T, Index>(&self) -> &T
    where
        Executors: crate::storage::Selector<T, Index>,
    {
        self.executors.get()
    }

    /// Set the event sender for routing events back to Core
    ///
    /// This allows Commands executed by Shell to send events back to Core for processing.
    #[must_use]
    pub fn with_event_sender(mut self, event_sender: Sender<Event>) -> Self {
        self.event_tx = Some(event_sender);
        self
    }

    /// Get an effect sender for executing effects
    #[must_use]
    pub fn effect_sender(&self) -> Sender<CommandStep<Event, Effect>> {
        self.effect_tx.clone()
    }

    /// Execute a Command following Crux's pattern
    ///
    /// This processes the Command synchronously and routes outputs:
    /// - Effects go to the effect handler via effect channel
    /// - Events go back to Core via event channel
    ///
    /// Command processing is now synchronous - async effect execution happens in tick().
    pub fn dispatch(&mut self, command: Command<Event, Effect>) -> Result<(), ShellError> {
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
    ///
    /// Single effects are processed sequentially for predictable behavior.
    /// Use Group with Parallel mode explicitly when concurrent execution is needed.
    #[allow(clippy::needless_pass_by_value)]
    pub async fn tick<S>(&mut self, spawner: S) -> Result<bool, ShellError>
    where
        S: crate::spawn::Spawn,
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
                    self.execute_effect(effect).await;
                }
                CommandStep::Batch(effects) => {
                    did_work = true;
                    _effect_count += effects.len();
                    self.spawn_batch_effects(effects, &spawner);
                }
                CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None } => {
                    did_work = true;
                    _effect_count += effects.len();
                    self.spawn_parallel_effects(effects, &spawner);
                }
                CommandStep::Group { effects, mode, barrier, timeout_per } => {
                    did_work = true;
                    _effect_count += effects.len();
                    self.spawn_group(effects, mode, barrier, timeout_per, &spawner);
                }
                CommandStep::Event(_) => {
                    // Other command outputs should be handled by Core
                    // This shouldn't happen in normal operation
                    unreachable!()
                }
            }
        }

        // Also tick any registered executors (for executor-based effect handling)
        // Note: This is currently a placeholder - full executor integration TBD

        // Cleanup finished tasks
        let (_cleaned_count, _active_count) = self.cleanup_finished_tasks();

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

    /// Execute a single effect sequentially (no spawning)
    ///
    /// This processes effects one at a time for predictable ordering.
    /// For parallel execution, use CommandStep::Group with Parallel mode.
    async fn execute_effect(&self, effect: Effect) {
        #[cfg(feature = "tracing")]
        debug!("Processing effect sequentially");

        let ctx = EffectContext::new(
            self.event_tx.clone(),
            self.resources.clone(),
            self.executors.clone(),
        );
        let handler = self.effect_handler;
        let timeout = self.config.effect_timeout;

        match timeout {
            Some(dur) => {
                let _ = crate::timer::timeout(dur, handler.handle(effect, ctx)).await;
            }
            None => {
                handler.handle(effect, ctx).await;
            }
        }
    }

    /// Spawn batch effects with optional timeout
    fn spawn_batch_effects<S>(&self, effects: Vec<Effect>, spawner: &S)
    where
        S: crate::spawn::Spawn,
    {
        let resources = self.resources.clone();
        let event_tx = self.event_tx.clone();
        let timeout = self.config.effect_timeout;
        let handler = self.effect_handler;
        let executors = self.executors.clone();

        let seq_future = async move {
            let ctx = EffectContext::new(event_tx, resources, executors);
            for effect in effects {
                match timeout {
                    Some(dur) => {
                        let _ = crate::timer::timeout(dur, handler.handle(effect, ctx.clone())).await;
                    }
                    None => {
                        handler.handle(effect, ctx.clone()).await;
                    }
                }
            }
        };
        spawner.spawn(seq_future);
    }

    /// Spawn effects in parallel with optional timeout
    fn spawn_parallel_effects<S>(&self, effects: Vec<Effect>, spawner: &S)
    where
        S: crate::spawn::Spawn,
    {
        let resources = self.resources.clone();
        let event_tx = self.event_tx.clone();
        let timeout = self.config.effect_timeout;
        let handler = self.effect_handler.clone();

        for effect in effects {
            let ctx = EffectContext::new(event_tx.clone(), resources.clone(), self.executors.clone());
            if let Some(dur) = timeout {
                let h = handler.clone();
                spawner.spawn(async move {
                    let _ = crate::timer::timeout(dur, h.handle(effect, ctx)).await;
                });
            } else {
                let h = handler.clone();
                spawner.spawn(async move {
                    h.handle(effect, ctx).await;
                });
            }
        }
    }

    /// Spawn a unified group with policy and optional barrier using futures_concurrency
    fn spawn_group<S>(
        &self,
        effects: Vec<Effect>,
        mode: crate::command::GroupMode,
        barrier: Option<Event>,
        timeout_per: Option<std::time::Duration>,
        spawner: &S,
    ) where
        S: crate::spawn::Spawn,
        H: Clone,
    {
        if effects.is_empty() {
            return;
        }

        let resources = self.resources.clone();
        let event_tx = self.event_tx.clone();
        let executors = self.executors.clone();
        let handler = self.effect_handler.clone();

        match mode {
            crate::command::GroupMode::Parallel => {
                // Barrier present => await all; None => fire-and-forget
                if barrier.is_none() {
                    // Equivalent to Group with Parallel mode
                    self.spawn_parallel_effects(effects, spawner);
                    return;
                }

                let barrier_event = barrier.unwrap();
                spawner.spawn(async move {
                    // Build child futures without pre-spawn; wrap with timeout if set
                    let futures: Vec<futures_util::future::BoxFuture<'static, ()>> = effects
                        .into_iter()
                        .map(|effect| {
                            let ctx = EffectContext::new(event_tx.clone(), resources.clone(), executors.clone());
                            let h = handler.clone();
                            async move {
                                let fut = h.handle(effect, ctx);
                                if let Some(d) = timeout_per {
                                    let _ = crate::timer::timeout(d, fut).await;
                                } else {
                                    fut.await;
                                }
                            }
                            .boxed()
                        })
                        .collect();
                    // Await all via futures_concurrency join
                    let _: Vec<_> = futures.join().await;
                    if let Some(tx) = event_tx.as_ref() { let _ = tx.send(barrier_event); }
                });
            }
            crate::command::GroupMode::Race => {
                spawner.spawn(async move {
                    let futures: Vec<futures_util::future::BoxFuture<'static, ()>> = effects
                        .into_iter()
                        .map(|effect| {
                            let ctx = EffectContext::new(event_tx.clone(), resources.clone(), executors.clone());
                            let h = handler.clone();
                            async move {
                                let fut = h.handle(effect, ctx);
                                if let Some(d) = timeout_per {
                                    let _ = crate::timer::timeout(d, fut).await;
                                } else {
                                    fut.await;
                                }
                            }
                            .boxed()
                        })
                        .collect();
                    // Await first; losers canceled by dropping
                    let _ = futures.race().await;
                    if let (Some(ev), Some(tx)) = (barrier, event_tx.as_ref()) { let _ = tx.send(ev); }
                });
            }
        }
    }

    /// Clean up finished tasks and return (cleaned_count, active_count)
    fn cleanup_finished_tasks(&self) -> (usize, usize) {
        if let Ok(mut tracker) = self.task_tracker.lock() {
            let cleaned = tracker.active_count();
            tracker.cleanup_finished();
            let active = tracker.active_count();
            (cleaned, active)
        } else {
            (0, 0)
        }
    }

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

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Dummy, // Just for the shell test - not actually used
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEffect {
        Log,
    }

    #[test]
    fn test_shell_creation() {
        let shell = Shell::<TestEvent, TestEffect>::new();

        assert_eq!(shell.task_stats().active_tasks, 0);
        assert!(!shell.task_stats().is_closed);
    }

    #[test]
    fn test_shell_resource_access() {
        use crate::builder::Syzygy;
        use crate::storage::{EmptyStorage, Storage};

        // Define a simple read-only resource
        #[derive(Debug, Clone)]
        struct HttpClient {
            base_url: String,
        }

        impl HttpClient {
            fn new() -> Self {
                Self {
                    base_url: "https://api.example.com".to_string(),
                }
            }
        }

        // Create shell using builder pattern
        let (_core, shell) = Syzygy::builder::<TestEvent, ()>()
            .model(()) // Need at least one model for Core
            .resource(HttpClient::new())
            .event_handler(
                |_event: TestEvent,
                 _ctx: &mut crate::event_context::EventContext<
                    TestEvent,
                    (),
                    Storage<(), EmptyStorage>,
                >| crate::command::Command::none(),
            )
            .effect_handler(())
            .build();

        // Test resource access
        let client: &HttpClient = shell.resource();
        assert_eq!(client.base_url, "https://api.example.com");

        // Test type inference
        let client_inferred: &HttpClient = shell.resource();
        assert_eq!(client_inferred.base_url, "https://api.example.com");
    }

    #[test]
    fn test_shell_multiple_resources() {
        use crate::builder::Syzygy;
        use crate::storage::{EmptyStorage, Storage};

        // Define multiple resource types
        #[derive(Debug, Clone)]
        struct HttpClient {
            base_url: String,
        }

        #[derive(Debug, Clone)]
        struct Database {
            connection_string: String,
        }

        #[derive(Debug, Clone)]
        struct FileSystem {
            root_path: String,
        }

        // Create shell with multiple resources using builder
        let (_core, shell) = Syzygy::builder::<TestEvent, ()>()
            .model(()) // Need at least one model for Core
            .resource(HttpClient {
                base_url: "https://api.example.com".to_string(),
            })
            .resource(Database {
                connection_string: "postgres://localhost".to_string(),
            })
            .resource(FileSystem {
                root_path: "/var/data".to_string(),
            })
            .event_handler(
                |_event: TestEvent,
                 _ctx: &mut crate::event_context::EventContext<
                    TestEvent,
                    (),
                    Storage<(), EmptyStorage>,
                >| crate::command::Command::none(),
            )
            .effect_handler(())
            .build();

        // Test accessing different resource types
        let client: &HttpClient = shell.resource();
        assert_eq!(client.base_url, "https://api.example.com");

        let db: &Database = shell.resource();
        assert_eq!(db.connection_string, "postgres://localhost");

        let fs: &FileSystem = shell.resource();
        assert_eq!(fs.root_path, "/var/data");
    }

    #[test]
    fn test_shell_resource_with_interior_mutability() {
        use crate::builder::Syzygy;
        use crate::storage::{EmptyStorage, Storage};
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        // Resource that needs interior mutability
        #[derive(Debug, Clone)]
        struct Cache {
            data: Arc<Mutex<HashMap<String, String>>>,
        }

        impl Cache {
            fn new() -> Self {
                Self {
                    data: Arc::new(Mutex::new(HashMap::new())),
                }
            }

            fn insert(&self, key: String, value: String) {
                self.data.lock().unwrap().insert(key, value);
            }

            fn get(&self, key: &str) -> Option<String> {
                self.data.lock().unwrap().get(key).cloned()
            }
        }

        // Create shell with cache resource using builder
        let (_core, shell) = Syzygy::builder::<TestEvent, ()>()
            .model(()) // Need at least one model for Core
            .resource(Cache::new())
            .event_handler(
                |_event: TestEvent,
                 _ctx: &mut crate::event_context::EventContext<
                    TestEvent,
                    (),
                    Storage<(), EmptyStorage>,
                >| crate::command::Command::none(),
            )
            .effect_handler(())
            .build();

        // Access cache resource (immutable reference)
        let cache: &Cache = shell.resource();

        // Use interior mutability to modify cache
        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());

        // Verify data was stored
        assert_eq!(cache.get("key1"), Some("value1".to_string()));
        assert_eq!(cache.get("key2"), Some("value2".to_string()));
        assert_eq!(cache.get("nonexistent"), None);
    }
}
