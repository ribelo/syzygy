use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::command::Command;
use crate::core::{Core, EventHandlerFn};
use crate::error::ShellError;
use crate::executor::Task;
use crate::extract::{EffectContext, EventContext, SubscriptionContext};
use crate::resource::ResourceMap;
use crate::shell::{EffectHandlerFn, Shell};
use crate::subscription::{Subscription, SubscriptionDriver, SubscriptionDrivers};
use crate::syzygy::{Syzygy, SyzygyConfig, UnhandledEffectPolicy};

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
type SubscriptionHandlerFn<E, X, M> = Box<dyn Fn(&SubscriptionContext<M>) -> Subscription<E, X>>;

pub struct SyzygyBuilder<E, X, M = ()>
where
    E: 'static,
    X: 'static,
{
    model: M,
    resources: ResourceMap,
    runtime: Option<crate::runtime::Runtime>,
    subscription_drivers: SubscriptionDrivers,
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
            runtime: None,
            subscription_drivers: SubscriptionDrivers::new(),
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
            runtime: self.runtime,
            subscription_drivers: self.subscription_drivers,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn with_resource<T: Clone + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
    }

    #[must_use]
    pub fn with_runtime(mut self, runtime: crate::runtime::Runtime) -> Self {
        self.runtime = Some(runtime);
        self
    }

    #[must_use]
    pub fn with_subscription_driver<D>(mut self, driver: D) -> Self
    where
        D: SubscriptionDriver,
    {
        self.subscription_drivers.register(driver);
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
            subscription_handler: None,
            model: self.model,
            resources: self.resources,
            runtime: self.runtime,
            subscription_drivers: self.subscription_drivers,
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
    subscription_handler: Option<SubscriptionHandlerFn<Event, Effect, Model>>,
    model: Model,
    resources: ResourceMap,
    runtime: Option<crate::runtime::Runtime>,
    subscription_drivers: SubscriptionDrivers,
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
    pub fn with_runtime(mut self, runtime: crate::runtime::Runtime) -> Self {
        self.runtime = Some(runtime);
        self
    }

    #[must_use]
    pub fn with_subscription_driver<D>(mut self, driver: D) -> Self
    where
        D: SubscriptionDriver,
    {
        self.subscription_drivers.register(driver);
        self
    }

    #[must_use]
    pub fn subscription_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(&SubscriptionContext<Model>) -> Subscription<Event, Effect> + 'static,
    {
        assert!(
            self.subscription_handler.is_none(),
            "subscription handler is already configured"
        );
        self.subscription_handler = Some(Box::new(handler));
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

        let routed_effect_handler = self
            .effect_handler
            .unwrap_or_else(|| Rc::new(|_effect, _ctx: &EffectContext<'_>| EffectRoute::Unhandled));
        let unhandled_effects_policy =
            Rc::new(Cell::new(self.syzygy_config.diagnostics.unhandled_effects));
        let unhandled_effects_for_handler = Rc::clone(&unhandled_effects_policy);

        let effect_handler: EffectHandlerFn<Event, Effect> =
            Rc::new(
                move |effect, ctx| match routed_effect_handler(effect, ctx) {
                    EffectRoute::Handled(task) => Ok(task),
                    EffectRoute::Unhandled => match unhandled_effects_for_handler.get() {
                        UnhandledEffectPolicy::Error => Err(ShellError::UnhandledEffect {
                            effect_type: std::any::type_name::<Effect>(),
                        }),
                        UnhandledEffectPolicy::Ignore => Ok(Task::none()),
                    },
                },
            );

        let runtime = match self.runtime {
            Some(runtime) => runtime,
            None => crate::runtime::Runtime::new()
                .map_err(|err| ShellError::RuntimeInitializationFailed(err.to_string()))?,
        };
        let subscription_handler = self.subscription_handler.map(Rc::from);

        let shell = Shell::with_subscriptions(
            event_tx,
            effect_handler,
            subscription_handler,
            self.subscription_drivers,
            self.resources,
            runtime,
            unhandled_effects_policy,
        );
        Ok(Syzygy::with_config(core, shell, self.syzygy_config))
    }
}
