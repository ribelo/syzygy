use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::command::Command;
use crate::core::{Core, EventHandlerFn};
use crate::error::ShellError;
use crate::executor::Task;
use crate::extract::{EffectContext, EventContext, SubscriptionContext};
use crate::runner_tester::RunnerTester;
use crate::shell::{BootHandlerFn, EffectHandlerFn, LifecycleHandlers, Shell};
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

type RoutedEffectHandlerFn<E, X, Resources> =
    Rc<dyn for<'a> Fn(X, &EffectContext<'a, Resources>) -> EffectRoute<E, X>>;
type BuilderBootHandlerFn<E, X, M> = BootHandlerFn<E, X, M>;
type SubscriptionHandlerFn<E, X, M> = Box<dyn Fn(&SubscriptionContext<M>) -> Subscription<E, X>>;

pub struct SyzygyBuilder<E, X, M = (), Resources = ()>
where
    E: 'static,
    X: 'static,
{
    model: M,
    resources: Resources,
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
            resources: (),
            runtime: None,
            subscription_drivers: SubscriptionDrivers::new(),
            _marker: PhantomData,
        }
    }
}

impl<Event, Effect, Model, Resources> SyzygyBuilder<Event, Effect, Model, Resources>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
    Resources: 'static,
{
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M, Resources> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            runtime: self.runtime,
            subscription_drivers: self.subscription_drivers,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn resources<R2: 'static>(self, resources: R2) -> SyzygyBuilder<Event, Effect, Model, R2> {
        SyzygyBuilder {
            model: self.model,
            resources,
            runtime: self.runtime,
            subscription_drivers: self.subscription_drivers,
            _marker: PhantomData,
        }
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
    pub fn event_handler<H>(self, handler: H) -> ConfiguredBuilder<Event, Effect, Model, Resources>
    where
        H: Fn(Event, &EventContext<Model>) -> Command<Event, Effect> + 'static,
    {
        ConfiguredBuilder {
            event_handler: Box::new(handler),
            effect_handler: None,
            boot_handler: None,
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

pub struct ConfiguredBuilder<Event, Effect, Model, Resources = ()>
where
    Event: 'static,
    Effect: 'static,
{
    event_handler: EventHandlerFn<Event, Effect, Model>,
    effect_handler: Option<RoutedEffectHandlerFn<Event, Effect, Resources>>,
    boot_handler: Option<BuilderBootHandlerFn<Event, Effect, Model>>,
    subscription_handler: Option<SubscriptionHandlerFn<Event, Effect, Model>>,
    model: Model,
    resources: Resources,
    runtime: Option<crate::runtime::Runtime>,
    subscription_drivers: SubscriptionDrivers,
    event_channel_capacity: Option<usize>,
    syzygy_config: SyzygyConfig,
    _marker: PhantomData<Effect>,
}

impl<Event, Effect, Model, Resources> ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: 'static,
    Effect: 'static,
    Model: 'static,
    Resources: 'static,
{
    #[must_use]
    pub fn effect_handler<H, R>(mut self, handler: H) -> Self
    where
        H: for<'a> Fn(Effect, &EffectContext<'a, Resources>) -> R + 'static,
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
        H: for<'a> Fn(Effect, &EffectContext<'a, Resources>) -> R + 'static,
        R: IntoEffectRoute<Event, Effect> + 'static,
        Effect: Clone,
    {
        let chained: RoutedEffectHandlerFn<Event, Effect, Resources> =
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
    pub fn resources<R2: 'static>(
        self,
        resources: R2,
    ) -> ConfiguredBuilder<Event, Effect, Model, R2> {
        assert!(
            self.effect_handler.is_none(),
            "resources must be configured before effect handlers"
        );
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: None,
            boot_handler: self.boot_handler,
            subscription_handler: self.subscription_handler,
            model: self.model,
            resources,
            runtime: self.runtime,
            subscription_drivers: self.subscription_drivers,
            event_channel_capacity: self.event_channel_capacity,
            syzygy_config: self.syzygy_config,
            _marker: PhantomData,
        }
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
    pub fn boot_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(&Model) -> Command<Event, Effect> + 'static,
    {
        assert!(
            self.boot_handler.is_none(),
            "boot handler is already configured"
        );
        self.boot_handler = Some(Box::new(handler));
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

    pub fn build(self) -> Result<Syzygy<Event, Effect, Model, Resources>, ShellError> {
        let (core, event_tx) = Core::with_event_channel_capacity(
            self.event_handler,
            self.model,
            self.event_channel_capacity,
        );

        let routed_effect_handler = self.effect_handler.unwrap_or_else(|| {
            Rc::new(|_effect, _ctx: &EffectContext<'_, Resources>| EffectRoute::Unhandled)
        });
        let unhandled_effects_policy =
            Rc::new(Cell::new(self.syzygy_config.diagnostics.unhandled_effects));
        let deferred_event_overflow_policy = Rc::new(Cell::new(
            self.syzygy_config.diagnostics.deferred_event_overflow,
        ));
        let unhandled_effects_for_handler = Rc::clone(&unhandled_effects_policy);

        let effect_handler: EffectHandlerFn<Event, Effect, Resources> = Rc::new(
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
            LifecycleHandlers::new(self.boot_handler, subscription_handler),
            self.subscription_drivers,
            self.resources,
            runtime,
            unhandled_effects_policy,
            deferred_event_overflow_policy,
        );
        Ok(Syzygy::with_config(core, shell, self.syzygy_config))
    }

    pub fn build_tester(
        mut self,
    ) -> Result<RunnerTester<Event, Effect, Model, Resources>, ShellError> {
        let (runtime, clock) = crate::runtime::Runtime::manual()
            .map_err(|err| ShellError::RuntimeInitializationFailed(err.to_string()))?;
        self.runtime = Some(runtime);
        let runner = self.build()?;
        Ok(RunnerTester::new(runner, clock))
    }
}
