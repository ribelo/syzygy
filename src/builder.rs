//! Builder for composing Core, Shell, resources, and executors.
//!
//! The builder wires your pure event handler to the async effect handler and
//! registers any executors you want the Shell to use. Keep models and resources
//! cheap to move/clone; the Shell clones resources for each effect call.
//!
//! This module intentionally avoids traits and lifetimes in the public surface
//! so usage stays straightforward in real apps and tests.
use crate::activity::Activity;
use crate::command::Command;
use crate::core::{Core, EventHandler, EventSender};
use crate::executor::{
    AsyncExecutor, BlockingExecutor, ExecutorRegistry, PanicDetails, PanicHook,
    ResourceBlockingExecutor, Task,
};
use crate::extract::EffectContext;
use crate::shell::{EffectHandlerFn, Shell, ShellStats};
use crate::syzygy::{Syzygy, SyzygyConfig};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

/// Entry point for building a `Syzygy`.
///
/// Typical flow:
/// - set `.model(..)` and optional `.with_resources(..)`
/// - install `.event_handler(..)` and `.effect_handler(..)`
/// - configure channel capacities and idle cadence via explicit builder knobs
/// - register executors via `.with_*_executor(..)`
/// - call `.build()`
pub struct SyzygyBuilder<E, X, M, R = ()>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    model: M,
    resources: R,
    _marker: PhantomData<(E, X)>,
}

impl<E, X> Default for SyzygyBuilder<E, X, (), ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, X> SyzygyBuilder<E, X, (), ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: (),
            resources: (),
            _marker: PhantomData,
        }
    }
}

impl<Event, Effect, Model, Resources> SyzygyBuilder<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
    Resources: Clone + Send + 'static,
{
    /// Replace the current model with a new one.
    ///
    /// Models are owned by `Core` and mutated only by your event handler.
    /// Calling `.model(..)` more than once replaces the previous value—compose
    /// your application state inside a single struct or tuple if you need
    /// multiple parts of state.
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M, Resources> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            _marker: PhantomData,
        }
    }

    /// Install application resources that all effects can access.
    ///
    /// The Shell clones `Resources` per effect call. Use `Arc<_>` for heavy
    /// dependencies (DB pools, clients) or wrap interior mutability explicitly.
    #[must_use]
    pub fn with_resources<R2>(self, resources: R2) -> SyzygyBuilder<Event, Effect, Model, R2>
    where
        R2: Clone + Send + 'static,
    {
        SyzygyBuilder {
            model: self.model,
            resources,
            _marker: PhantomData,
        }
    }

    /// Finalize model configuration and set the event handler.
    ///
    /// Your event handler is a pure function. It mutates the model and returns
    /// a `Command` telling the Shell what effects to run.
    #[must_use]
    pub fn event_handler(
        self,
        event_handler: EventHandler<Event, Effect, Model>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resources> {
        ConfiguredBuilder {
            event_handler,
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            exec_registry: ExecutorRegistry::<Event>::default(),
            effect_channel_capacity: None,
            event_channel_capacity: None,
            syzygy_config: SyzygyConfig::default(),
            panic_handler: None,
            _marker: PhantomData,
        }
    }
}

/// Builder stage where handlers and executors are configured.
///
/// After installing handlers you can register any number of executors. If a
/// `Task` targets a specific executor type you didn’t register, the Shell will
/// error when scheduling it. You can avoid registry lookups entirely by using
/// Register executors explicitly; pair `Task::async_on` with the executor types you add via the builder.
/// runtime if available; otherwise completes inline by blocking the thread).
pub struct ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    event_handler: EventHandler<Event, Effect, Model>,
    effect_handler: Option<EffectHandlerFn<Event, Effect, Resources>>,
    model: Model,
    resources: Resources,
    exec_registry: ExecutorRegistry<Event>,
    effect_channel_capacity: Option<usize>,
    event_channel_capacity: Option<usize>,
    syzygy_config: SyzygyConfig,
    panic_handler: Option<Arc<PanicHook<Event, Effect>>>,
    _marker: PhantomData<Effect>,
}

impl<Event, Effect, Model, Resources> ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
    Resources: Clone + Send + 'static,
{
    /// Install the effect dispatch function.
    ///
    /// Match on effect variants and call `.handle(payload, ctx)` on individual
    /// handlers. Each handler extracts its own resources via
    /// [`FromEffectContext`](crate::extract::FromEffectContext).
    ///
    /// ```ignore
    /// .effect_handler(|effect, ctx| match effect {
    ///     Effect::Save(data) => save.handle(data, ctx),
    ///     Effect::Notify(msg) => notify.handle(msg, ctx),
    /// })
    /// ```
    #[must_use]
    pub fn effect_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(Effect, &EffectContext<Resources>) -> Task<Event, Effect> + Send + 'static,
    {
        self.effect_handler = Some(Box::new(handler));
        self
    }

    /// Apply a prebuilt runner configuration.
    #[must_use]
    pub fn with_syzygy_config(mut self, config: SyzygyConfig) -> Self {
        self.syzygy_config = config;
        self
    }

    /// Install a handler invoked whenever a task panics.
    ///
    /// The handler receives [`PanicDetails`] describing the task context and a message captured
    /// from the panic payload. Return a `Command` (typically an error event) to surface the
    /// failure to your Core.
    #[must_use]
    pub fn with_panic_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(PanicDetails, String) -> Command<Event, Effect> + Send + Sync + 'static,
    {
        self.panic_handler = Some(Arc::new(handler));
        self
    }

    /// Replace the executor registry with a pre-built one.
    #[must_use]
    pub fn with_executor_registry(mut self, registry: ExecutorRegistry<Event>) -> Self {
        self.exec_registry = registry;
        self
    }

    /// Register an async executor.
    ///
    /// Use for futures/streams scheduling (e.g. `TokioExecutor`, `InlineAsync`).
    #[must_use]
    pub fn with_async_executor<T>(mut self, exec: T) -> Self
    where
        T: AsyncExecutor + Send + Sync + 'static,
    {
        self.exec_registry.insert_async(exec);
        self
    }

    /// Register a blocking executor.
    ///
    /// Use for CPU-bound or blocking work that doesn’t require a shared
    /// resource (e.g. Rayon-based executor).
    #[must_use]
    pub fn with_blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: BlockingExecutor + Send + Sync + 'static,
    {
        self.exec_registry.insert_blocking(exec);
        self
    }

    /// Register a resource-blocking executor (single-threaded shared resource).
    ///
    /// Use when jobs need mutable access to a single owned resource with FIFO
    /// guarantees (e.g. a device handle that must not be used concurrently).
    #[must_use]
    pub fn with_resource_blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: ResourceBlockingExecutor + Send + Sync + 'static,
    {
        self.exec_registry.insert_resource_blocking(exec);
        self
    }

    /// Override the Shell effect channel capacity.
    ///
    /// `None` means unbounded. Bounded channels apply backpressure to the
    /// caller when the effect queue is saturated by returning
    /// [`ShellError::EffectQueueFull`](crate::error::ShellError::EffectQueueFull).
    /// Combine with [`Self::with_syzygy_config`] to tune idle cadence alongside channel sizing.
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # fn update(_: Event, _: &mut Model) -> Command<Event, Effect> { Command::none() }
    /// # fn effects(_: Effect, _: ()) -> Task<Event, Effect> { Task::none() }
    /// # #[derive(Default)] struct Model;
    /// # #[derive(Clone)] enum Event { Ping }
    /// # #[derive(Clone)] enum Effect { DoPing }
    /// let runner = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(update)
    ///     .effect_handler(effects)
    ///     .with_effect_channel_capacity(Some(256))
    ///     .build();
    /// # let _ = runner;
    /// ```
    #[must_use]
    pub fn with_effect_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.effect_channel_capacity = capacity;
        self
    }

    /// Override the core event channel capacity.
    ///
    /// `None` keeps the default unbounded channel. Bounded channels apply backpressure
    /// to event producers by returning [`CoreError::ChannelFull`](crate::error::CoreError::ChannelFull).
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # fn update(_: Event, _: &mut Model) -> Command<Event, Effect> { Command::none() }
    /// # fn effects(_: Effect, _: ()) -> Task<Event, Effect> { Task::none() }
    /// # #[derive(Default)] struct Model;
    /// # #[derive(Clone)] enum Event { Ping }
    /// # #[derive(Clone)] enum Effect { DoPing }
    /// let runner = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(update)
    ///     .effect_handler(effects)
    ///     .with_event_channel_capacity(Some(64))
    ///     .build();
    /// # let _ = runner;
    /// ```
    #[must_use]
    pub fn with_event_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Build the system and return a `Syzygy`.
    pub fn build(self) -> Syzygy<Event, Effect, Model, Resources> {
        let (core, event_tx) = Core::with_event_channel_capacity(
            self.event_handler,
            self.model,
            self.event_channel_capacity,
        );
        let registry = Arc::new(self.exec_registry);

        let shell = Self::build_shell(
            registry,
            self.effect_handler,
            self.resources,
            event_tx,
            self.effect_channel_capacity,
            self.panic_handler,
        );
        Syzygy::with_config(core, shell, self.syzygy_config)
    }

    fn build_shell(
        exec_registry: Arc<ExecutorRegistry<Event>>,
        effect_handler: Option<EffectHandlerFn<Event, Effect, Resources>>,
        resources: Resources,
        event_tx: EventSender<Event>,
        effect_channel_capacity: Option<usize>,
        panic_handler: Option<Arc<PanicHook<Event, Effect>>>,
    ) -> Shell<Event, Effect, Resources> {
        use crossbeam_channel::{bounded, unbounded};

        let (effect_tx, effect_rx) = match effect_channel_capacity {
            Some(capacity) => bounded(capacity),
            None => unbounded(),
        };

        let effect_handler: EffectHandlerFn<Event, Effect, Resources> =
            effect_handler.unwrap_or_else(|| {
                Box::new(|_effect, _ctx: &EffectContext<Resources>| Task::<Event, Effect>::none())
            });

        Shell {
            effect_rx,
            effect_tx,
            event_tx,
            effect_handler,
            resources,
            activity: Activity::new(),
            effect_channel_capacity,
            executors: exec_registry,
            closed: false,
            prefetched_effects: VecDeque::new(),
            stats: ShellStats::default(),
            executors_shutdown: false,
            panic_handler,
        }
    }
}

