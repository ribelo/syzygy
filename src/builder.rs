//! Builder for composing Core, Shell, resources, and executors.
//!
//! The builder wires your pure event handler to the async effect handler and
//! registers any executors you want the Shell to use.
//!
//! This module intentionally avoids traits and lifetimes in the public surface
//! so usage stays straightforward in real apps and tests.
use crate::activity::Activity;
use crate::command::Command;
use crate::core::{Core, EventHandlerFn};
use crate::dependency::ResourceMap;
use crate::executor::{AsyncExecutor, BlockingExecutor, PanicDetails, PanicHook, Task};
use crate::extract::{EffectContext, EventContext};
use crate::reducer::Reducer;
use crate::shell::{EffectHandlerFn, Shell, ShellStats};
use crate::syzygy::{Syzygy, SyzygyConfig};
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

/// Entry point for building a `Syzygy`.
///
/// Typical flow:
/// - set `.model(..)` and optional `.with_resource(..)` values
/// - install `.event_handler(..)` and `.effect_handler(..)`
/// - configure channel capacities and idle cadence via explicit builder knobs
/// - register executors via `.async_executor(..)`, `.compute_executor(..)`, and `.blocking_executor(..)`
/// - call `.build()`
pub struct SyzygyBuilder<E, X, M = ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    model: M,
    resources: ResourceMap,
    _marker: PhantomData<(E, X)>,
}

impl<E, X> Default for SyzygyBuilder<E, X, ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, X> SyzygyBuilder<E, X, ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: (),
            resources: ResourceMap::new(),
            _marker: PhantomData,
        }
    }
}

impl<Event, Effect, Model> SyzygyBuilder<Event, Effect, Model>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
{
    /// Replace the current model with a new one.
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            _marker: PhantomData,
        }
    }

    /// Register a typed resource available to all effect handlers via `Res<T>`.
    #[must_use]
    pub fn with_resource<T: Send + Sync + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
    }

    /// Alias for [`Self::with_resource`].
    #[must_use]
    pub fn with_dependency<T: Send + Sync + 'static>(self, resource: T) -> Self {
        self.with_resource(resource)
    }

    /// Install the event dispatch function.
    #[must_use]
    pub fn event_handler<H>(self, handler: H) -> ConfiguredBuilder<Event, Effect, Model>
    where
        H: Fn(Event, &EventContext<Model>) -> Command<Event, Effect> + Send + 'static,
    {
        ConfiguredBuilder {
            event_handler: Box::new(handler),
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            async_executor: None,
            compute_executor: None,
            blocking_executor: None,
            effect_channel_capacity: None,
            event_channel_capacity: None,
            syzygy_config: SyzygyConfig::default(),
            panic_handler: None,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn reducer<R>(self, reducer: R) -> ConfiguredBuilder<Event, Effect, Model>
    where
        R: Reducer<State = Model, Event = Event, Effect = Effect> + Send + 'static,
    {
        self.event_handler(move |event, ctx| reducer.reduce(event, ctx))
    }
}

/// Builder stage where handlers and executors are configured.
pub struct ConfiguredBuilder<Event, Effect, Model>
where
    Event: Send + 'static,
    Effect: Send + 'static,
{
    event_handler: EventHandlerFn<Event, Effect, Model>,
    effect_handler: Option<EffectHandlerFn<Event, Effect>>,
    model: Model,
    resources: ResourceMap,
    async_executor: Option<Arc<dyn AsyncExecutor>>,
    compute_executor: Option<Arc<dyn BlockingExecutor>>,
    blocking_executor: Option<Arc<dyn BlockingExecutor>>,
    effect_channel_capacity: Option<usize>,
    event_channel_capacity: Option<usize>,
    syzygy_config: SyzygyConfig,
    panic_handler: Option<Arc<PanicHook<Event, Effect>>>,
    _marker: PhantomData<Effect>,
}

impl<Event, Effect, Model> ConfiguredBuilder<Event, Effect, Model>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
{
    /// Install the effect dispatch function.
    #[must_use]
    pub fn effect_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(Effect, &EffectContext) -> Task<Event, Effect> + Send + 'static,
    {
        self.effect_handler = Some(Box::new(handler));
        self
    }

    /// Register a typed resource available to all effect handlers via `Res<T>`.
    #[must_use]
    pub fn with_resource<T: Send + Sync + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
    }

    /// Alias for [`Self::with_resource`].
    #[must_use]
    pub fn with_dependency<T: Send + Sync + 'static>(self, resource: T) -> Self {
        self.with_resource(resource)
    }

    /// Apply a prebuilt runner configuration.
    #[must_use]
    pub fn with_syzygy_config(mut self, config: SyzygyConfig) -> Self {
        self.syzygy_config = config;
        self
    }

    /// Install a handler invoked whenever a task panics.
    #[must_use]
    pub fn with_panic_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(PanicDetails, String) -> Command<Event, Effect> + Send + Sync + 'static,
    {
        self.panic_handler = Some(Arc::new(handler));
        self
    }

    /// Configure the async executor used for futures and streams.
    #[must_use]
    pub fn async_executor<T>(mut self, exec: T) -> Self
    where
        T: AsyncExecutor + Send + Sync + 'static,
    {
        self.async_executor = Some(Arc::new(exec));
        self
    }

    /// Configure the compute executor used for CPU-bound work.
    #[must_use]
    pub fn compute_executor<T>(mut self, exec: T) -> Self
    where
        T: BlockingExecutor + Send + Sync + 'static,
    {
        self.compute_executor = Some(Arc::new(exec));
        self
    }

    /// Configure the blocking executor used for blocking I/O work.
    #[must_use]
    pub fn blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: BlockingExecutor + Send + Sync + 'static,
    {
        self.blocking_executor = Some(Arc::new(exec));
        self
    }

    /// Override the Shell effect channel capacity.
    #[must_use]
    pub fn with_effect_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.effect_channel_capacity = capacity;
        self
    }

    /// Override the core event channel capacity.
    #[must_use]
    pub fn with_event_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Build the system and return a `Syzygy`.
    pub fn build(self) -> Syzygy<Event, Effect, Model> {
        use crossbeam_channel::{bounded, unbounded};

        let (core, event_tx) = Core::with_event_channel_capacity(
            self.event_handler,
            self.model,
            self.event_channel_capacity,
        );

        let async_executor = if let Some(async_executor) = self.async_executor {
            async_executor
        } else {
            #[cfg(feature = "rt-inline")]
            {
                Arc::new(crate::executor::InlineAsync::new())
            }
            #[cfg(not(feature = "rt-inline"))]
            {
                panic!("no async executor configured")
            }
        };

        let (effect_tx, effect_rx) = match self.effect_channel_capacity {
            Some(capacity) => bounded(capacity),
            None => unbounded(),
        };

        let effect_handler: EffectHandlerFn<Event, Effect> =
            self.effect_handler.unwrap_or_else(|| {
                Box::new(|_effect, _ctx: &EffectContext| Task::<Event, Effect>::none())
            });

        let shell = Shell {
            effect_rx,
            effect_tx,
            event_tx,
            effect_handler,
            resources: self.resources,
            activity: Activity::new(),
            cancel_generations: Arc::new(Mutex::new(HashMap::new())),
            effect_channel_capacity: self.effect_channel_capacity,
            async_executor,
            compute_executor: self.compute_executor,
            blocking_executor: self.blocking_executor,
            closed: false,
            prefetched_effects: VecDeque::new(),
            stats: ShellStats::default(),
            executors_shutdown: false,
            panic_handler: self.panic_handler,
        };

        Syzygy::with_config(core, shell, self.syzygy_config)
    }
}
