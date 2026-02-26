use std::marker::PhantomData;
use std::rc::Rc;

use crate::command::Command;
use crate::core::{Core, EventHandlerFn};
use crate::dependency::ResourceMap;
use crate::executor::Task;
use crate::extract::{EffectContext, EventContext};
use crate::feature::Feature;
use crate::reducer::Reducer;
use crate::shell::{EffectHandlerFn, Shell};
use crate::syzygy::{Syzygy, SyzygyConfig};

pub struct SyzygyBuilder<E, X, M = ()>
where
    E: 'static,
    X: 'static,
{
    model: M,
    resources: ResourceMap,
    _marker: PhantomData<(E, X)>,
}

impl<E, X> Default for SyzygyBuilder<E, X, ()>
where
    E: 'static,
    X: 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, X> SyzygyBuilder<E, X, ()>
where
    E: 'static,
    X: 'static,
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
    Event: 'static,
    Effect: 'static,
    Model: 'static,
{
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn with_resource<T: Clone + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
    }

    #[must_use]
    pub fn with_dependency<T: Clone + 'static>(self, resource: T) -> Self {
        self.with_resource(resource)
    }

    #[must_use]
    pub fn event_handler<H>(self, handler: H) -> ConfiguredBuilder<Event, Effect, Model>
    where
        H: Fn(Event, &EventContext<Model>) -> Command<Event, Effect> + 'static,
    {
        ConfiguredBuilder {
            event_handler: Box::new(handler),
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            event_channel_capacity: None,
            syzygy_config: SyzygyConfig::default(),
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn reducer<R>(self, reducer: R) -> ConfiguredBuilder<Event, Effect, Model>
    where
        R: Reducer<State = Model, Event = Event, Effect = Effect> + 'static,
    {
        self.event_handler(move |event, ctx| reducer.reduce(event, ctx))
    }

    #[must_use]
    pub fn feature<F>(self, feature: F) -> ConfiguredBuilder<Event, Effect, Model>
    where
        F: Feature<State = Model, Event = Event, Effect = Effect> + 'static,
    {
        let feature = Rc::new(feature);
        let reducer_feature = Rc::clone(&feature);
        let effect_feature = Rc::clone(&feature);

        let mut configured =
            self.event_handler(move |event, ctx| reducer_feature.reduce(event, ctx));
        configured.effect_handler = Some(Rc::new(move |effect, ctx| {
            effect_feature.handle_effect(effect, ctx)
        }));
        configured
    }
}

pub struct ConfiguredBuilder<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
{
    event_handler: EventHandlerFn<Event, Effect, Model>,
    effect_handler: Option<EffectHandlerFn<Event, Effect>>,
    model: Model,
    resources: ResourceMap,
    event_channel_capacity: Option<usize>,
    syzygy_config: SyzygyConfig,
    _marker: PhantomData<Effect>,
}

impl<Event, Effect, Model> ConfiguredBuilder<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
{
    #[must_use]
    pub fn effect_handler<H>(mut self, handler: H) -> Self
    where
        H: for<'a> Fn(Effect, &EffectContext<'a>) -> Task<Event, Effect> + 'static,
    {
        self.effect_handler = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn with_resource<T: Clone + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
    }

    #[must_use]
    pub fn with_dependency<T: Clone + 'static>(self, resource: T) -> Self {
        self.with_resource(resource)
    }

    #[must_use]
    pub fn with_syzygy_config(mut self, config: SyzygyConfig) -> Self {
        self.syzygy_config = config;
        self
    }

    #[must_use]
    pub fn with_event_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Kept for backward compatibility. Syzygy now always uses compio.
    #[must_use]
    pub fn async_executor<T>(self, _executor: T) -> Self {
        self
    }

    /// Kept for backward compatibility. Blocking work should be spawned from async effects.
    #[must_use]
    pub fn compute_executor<T>(self, _executor: T) -> Self {
        self
    }

    /// Kept for backward compatibility. Blocking work should be spawned from async effects.
    #[must_use]
    pub fn blocking_executor<T>(self, _executor: T) -> Self {
        self
    }

    /// Kept for backward compatibility. No dedicated shell effect queue exists anymore.
    #[must_use]
    pub fn with_effect_channel_capacity(self, _capacity: Option<usize>) -> Self {
        self
    }

    pub fn build(self) -> Syzygy<Event, Effect, Model> {
        let (core, event_tx) = Core::with_event_channel_capacity(
            self.event_handler,
            self.model,
            self.event_channel_capacity,
        );

        let effect_handler = self
            .effect_handler
            .unwrap_or_else(|| Rc::new(|_effect, _ctx: &EffectContext<'_>| Task::none()));

        let shell = Shell::new(event_tx, effect_handler, self.resources);
        Syzygy::with_config(core, shell, self.syzygy_config)
    }
}
