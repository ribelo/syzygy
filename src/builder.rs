use std::marker::PhantomData;
use std::rc::Rc;

use crate::command::Command;
use crate::core::{Core, EventHandlerFn};
use crate::error::ShellError;
use crate::executor::Task;
use crate::extract::{EffectContext, EventContext};
use crate::resource::ResourceMap;
use crate::shell::{EffectHandlerFn, Shell};
use crate::syzygy::{Syzygy, SyzygyConfig};

/// Result of attempting to handle an effect in builder-level composition.
pub enum EffectRoute<E, X> {
    Handled(Task<E, X>),
    Unhandled,
}

/// Conversion trait for builder effect handlers.
///
/// Returning [`Task`] means the effect was handled. Returning
/// `Option<Task>` allows explicit fallthrough (`None`) in chained handlers.
pub trait IntoEffectRoute<E, X> {
    fn into_effect_route(self) -> EffectRoute<E, X>;
}

impl<E, X> IntoEffectRoute<E, X> for Task<E, X> {
    fn into_effect_route(self) -> EffectRoute<E, X> {
        EffectRoute::Handled(self)
    }
}

impl<E, X> IntoEffectRoute<E, X> for Option<Task<E, X>> {
    fn into_effect_route(self) -> EffectRoute<E, X> {
        match self {
            Some(task) => EffectRoute::Handled(task),
            None => EffectRoute::Unhandled,
        }
    }
}

type RoutedEffectHandlerFn<E, X> = Rc<dyn for<'a> Fn(X, &EffectContext<'a>) -> EffectRoute<E, X>>;

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
}

pub struct ConfiguredBuilder<Event, Effect, Model>
where
    Event: 'static,
    Effect: 'static,
{
    event_handler: EventHandlerFn<Event, Effect, Model>,
    effect_handler: Option<RoutedEffectHandlerFn<Event, Effect>>,
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
    pub fn effect_handler<H, R>(mut self, handler: H) -> Self
    where
        H: for<'a> Fn(Effect, &EffectContext<'a>) -> R + 'static,
        R: IntoEffectRoute<Event, Effect> + 'static,
    {
        assert!(
            self.effect_handler.is_none(),
            "effect handler is already configured; use .chain_effect_handler() to compose handlers"
        );
        self.effect_handler = Some(Rc::new(move |effect, ctx| {
            handler(effect, ctx).into_effect_route()
        }));
        self
    }

    #[must_use]
    pub fn chain_effect_handler<H, R>(mut self, handler: H) -> Self
    where
        H: for<'a> Fn(Effect, &EffectContext<'a>) -> R + 'static,
        R: IntoEffectRoute<Event, Effect> + 'static,
        Effect: Clone,
    {
        let chained: RoutedEffectHandlerFn<Event, Effect> =
            Rc::new(move |effect, ctx| handler(effect, ctx).into_effect_route());
        self.effect_handler = Some(match self.effect_handler.take() {
            Some(existing) => {
                let chained = Rc::clone(&chained);
                Rc::new(move |effect, ctx| match existing(effect.clone(), ctx) {
                    EffectRoute::Handled(task) => EffectRoute::Handled(task),
                    EffectRoute::Unhandled => chained(effect, ctx),
                })
            }
            None => chained,
        });
        self
    }

    #[must_use]
    pub fn with_resource<T: Clone + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
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

    pub fn build(self) -> Result<Syzygy<Event, Effect, Model>, ShellError> {
        let (core, event_tx) = Core::with_event_channel_capacity(
            self.event_handler,
            self.model,
            self.event_channel_capacity,
        );

        let routed_effect_handler = self.effect_handler.unwrap_or_else(|| {
            Rc::new(|_effect, _ctx: &EffectContext<'_>| EffectRoute::Handled(Task::none()))
        });

        let effect_handler: EffectHandlerFn<Event, Effect> =
            Rc::new(
                move |effect, ctx| match routed_effect_handler(effect, ctx) {
                    EffectRoute::Handled(task) => task,
                    EffectRoute::Unhandled => Task::none(),
                },
            );

        let runtime = crate::runtime::Runtime::new()
            .map_err(|err| ShellError::RuntimeInitializationFailed(err.to_string()))?;
        let shell = Shell::new(event_tx, effect_handler, self.resources, runtime);
        Ok(Syzygy::with_config(core, shell, self.syzygy_config))
    }
}
