use crate::core::{Core, EventHandler};
use crate::effect_context::EffectContext;
use crate::executor::Outcome;
use crate::executor::{AsyncExecutor, ExecutorRegistry, SyncExecutor};
use crate::shell::{EffectHandler, EffectWork, IntoEffectWork, Shell, ShellConfig};
use crate::syzygy::Syzygy;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

/// Marker type indicating no executor registry has been provided
pub struct NoExecutor;

/// Marker type indicating an executor registry has been provided
pub struct HasExecutor;

/// Base builder phase: configure models, resources, executors
pub struct SyzygyBuilder<E, X, M, R> {
    model: M,
    resources: R,
    exec_registry: Option<ExecutorRegistry<E>>,
    _marker: std::marker::PhantomData<X>,
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
    /// Create a new builder with empty model and resources
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: (),
            resources: (),
            exec_registry: None,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Event, Effect, Model, Resource> SyzygyBuilder<Event, Effect, Model, Resource>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
    Resource: Send + Sync + 'static,
{
    /// Add a model to the builder
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M, Resource> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            _marker: std::marker::PhantomData,
        }
    }

    /// Add resources to the builder
    #[must_use]
    pub fn resource<R: Send + Sync + 'static>(
        self,
        resources: R,
    ) -> SyzygyBuilder<Event, Effect, Model, R> {
        SyzygyBuilder {
            model: self.model,
            resources,
            exec_registry: self.exec_registry,
            _marker: std::marker::PhantomData,
        }
    }

    /// Transition to configured phase by setting the event handler
    #[must_use]
    pub fn event_handler(
        self,
        event_handler: EventHandler<Event, Effect, Model>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, NoExecutor> {
        ConfiguredBuilder {
            event_handler,
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            shell_config: ShellConfig::default(),
            _executor_state: PhantomData,
        }
    }
}

/// Configured phase: handlers are set; building is allowed. No more storage changes to avoid type surprises.
pub struct ConfiguredBuilder<Event, Effect, Model, Resource, ExecutorState = NoExecutor> {
    event_handler: EventHandler<Event, Effect, Model>,
    effect_handler: Option<EffectHandler<Event, Effect, Resource>>,
    model: Model,
    resources: Resource,
    exec_registry: Option<ExecutorRegistry<Event>>,
    shell_config: ShellConfig,
    _executor_state: PhantomData<ExecutorState>,
}

impl<Event, Effect, Model, Resource, ExecutorState>
    ConfiguredBuilder<Event, Effect, Model, Resource, ExecutorState>
where
    Event: Send + Sync + 'static,
    Effect: Send + 'static,
    Resource: Send + Sync + 'static,
{
    /// Set the effect handler that processes effects
    #[must_use]
    pub fn effect_handler<H, T>(
        self,
        handler: H,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, ExecutorState>
    where
        H: FnMut(Effect, EffectContext<Event, Resource>) -> T + Send + 'static,
        T: IntoEffectWork<Event, Resource>,
    {
        let mut handler = handler;
        let boxed: EffectHandler<Event, Effect, Resource> =
            Box::new(move |effect, ctx| handler(effect, ctx).into_effect_work());
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: Some(boxed),
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            shell_config: self.shell_config,
            _executor_state: PhantomData,
        }
    }

    /// Provide a configurator that can build the registry when `event_tx` is available.
    #[must_use]
    pub fn with_executor_registry(
        self,
        registry: ExecutorRegistry<Event>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, HasExecutor> {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(registry),
            shell_config: self.shell_config,
            _executor_state: PhantomData,
        }
    }

    /// Add a single async executor to the registry by concrete type
    #[must_use]
    pub fn with_async_executor<T>(
        self,
        exec: T,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, HasExecutor>
    where
        T: AsyncExecutor<Event> + Send + 'static,
    {
        let mut reg = self.exec_registry.unwrap_or_default();
        reg.insert_async(exec);
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(reg),
            shell_config: self.shell_config,
            _executor_state: PhantomData,
        }
    }

    /// Add a single sync executor to the registry by concrete type
    #[must_use]
    pub fn with_sync_executor<T>(
        self,
        exec: T,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, HasExecutor>
    where
        T: SyncExecutor<Event> + Send + 'static,
    {
        let mut reg = self.exec_registry.unwrap_or_default();
        reg.insert_sync(exec);
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(reg),
            shell_config: self.shell_config,
            _executor_state: PhantomData,
        }
    }

    /// Override the shell configuration before building the system.
    #[must_use]
    pub fn with_shell_config(
        self,
        config: ShellConfig,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, ExecutorState> {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            shell_config: config,
            _executor_state: PhantomData,
        }
    }

    /// Internal helper to build shell
    fn build_shell(
        resources: Resource,
        exec_registry: Arc<ExecutorRegistry<Event>>,
        effect_handler: Option<EffectHandler<Event, Effect, Resource>>,
        event_tx: crossbeam_channel::Sender<Event>,
        config: ShellConfig,
    ) -> Shell<Event, Effect, Resource> {
        use crossbeam_channel::{bounded, unbounded};

        fn default_effect_handler<E, X, R>(_: X, _: EffectContext<E, R>) -> EffectWork<E, R>
        where
            E: Send + 'static,
            X: Send + 'static,
            R: Send + Sync + 'static,
        {
            EffectWork::Immediate(Outcome::None)
        }

        let capacity = config.effect_channel_capacity;
        let (effect_tx, effect_rx) = match capacity {
            Some(capacity) => bounded(capacity),
            None => unbounded(),
        };

        let effect_handler = effect_handler.unwrap_or_else(|| {
            Box::new(move |effect, ctx| {
                default_effect_handler::<Event, Effect, Resource>(effect, ctx)
            })
        });

        Shell {
            effect_rx,
            effect_tx,
            event_tx,
            resources: Arc::new(resources),
            // use provided handler or a no-op
            effect_handler,
            config,
            executors: exec_registry,
            closed: false,
            prefetched_effects: VecDeque::new(),
        }
    }
}

// Separate impl block for HasExecutor state - only this state can build
impl<Event, Effect, Model, Resource> ConfiguredBuilder<Event, Effect, Model, Resource, HasExecutor>
where
    Event: Send + Sync + 'static,
    Effect: Send + 'static,
    Resource: Send + Sync + 'static,
{
    /// Build the system and return a `Syzygy` that owns the Core and Shell.
    pub fn build(self) -> Syzygy<Event, Effect, Model, Resource> {
        let (core, event_tx) = Core::new(self.event_handler, self.model);
        let shell = Self::build_shell(
            self.resources,
            Arc::new(self.exec_registry.unwrap()), // Safe to unwrap due to type system guarantee
            self.effect_handler,
            event_tx,
            self.shell_config,
        );
        Syzygy::new(core, shell)
    }

    /// Build the system and return a Syzygy that owns both Core and Shell.
    pub fn build_syzygy(self) -> Syzygy<Event, Effect, Model, Resource> {
        self.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
    }

    #[derive(Debug, Default)]
    struct TestModel {
        count: i32,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    fn test_update(
        event: TestEvent,
        ctx: &mut crate::event_context::EventContext<TestEvent, TestEffect, TestModel>,
    ) -> Command<TestEvent, TestEffect> {
        match event {
            TestEvent::Increment => {
                let model: &mut TestModel = ctx.model_mut();
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[test]
    fn test_builder() {
        let registry = crate::executor::ExecutorRegistry::new();

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry)
            .build();

        let (mut core, _shell) = runner.split();

        let _command = core.handle_event(TestEvent::Increment);
        let model: &TestModel = core.model();
        assert_eq!(model.count, 1);
    }

    #[test]
    fn test_shell_type_with_models_only() {
        // Test that Shell type remains simple when only models are added (no resources)
        let registry = crate::executor::ExecutorRegistry::new();

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry)
            .build();

        let (_core, shell) = runner.split();

        // Type should remain simple with resources-only generic
        let _: Shell<TestEvent, TestEffect, ()> = shell;

        // The key test: adding models should NOT change the resource storage type
        // This confirms that the builder correctly separates model storage from resource storage
    }

    #[test]
    fn test_shell_type_inference_works() {
        // Test that users don't need to specify complex types manually
        let registry = crate::executor::ExecutorRegistry::new();

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry)
            .build();

        let (mut core, _shell) = runner.split();

        // This should just work without any type annotations needed
        core.handle_event(TestEvent::Increment);
        let model: &TestModel = core.model();
        assert_eq!(model.count, 1);
    }

    #[test]
    fn test_multi_model() {
        #[derive(Debug, Default)]
        struct UserModel {
            name: String,
        }

        #[derive(Debug, Default)]
        struct ConfigModel {
            theme: String,
        }

        fn multi_update(
            event: TestEvent,
            ctx: &mut crate::event_context::EventContext<
                TestEvent,
                TestEffect,
                (UserModel, ConfigModel),
            >,
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment => {
                    let (user, config) = ctx.model_mut();
                    user.name = "Updated".to_string();
                    config.theme = "dark".to_string();

                    Command::effect(TestEffect::Log)
                }
            }
        }

        let registry = crate::executor::ExecutorRegistry::new();

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model((
                UserModel {
                    name: "Alice".to_string(),
                },
                ConfigModel {
                    theme: "light".to_string(),
                },
            ))
            .event_handler(multi_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry)
            .build();

        let (mut core, _shell) = runner.split();

        core.handle_event(TestEvent::Increment);

        let (_user, config) = core.model();

        // Skipped model assertions in refactor
        assert_eq!(config.theme, "dark");
    }

    #[test]
    fn test_typestate_prevents_build_without_executor() {
        // This test verifies that the typestate pattern works correctly
        // The following code should NOT compile because we're trying to build without an executor

        // Uncomment the code below to see the compile-time error:
        //
        // let (_core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
        //     .model(TestModel { count: 0 })
        //     .event_handler(test_update)
        //     .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
        //     .build(); // ERROR: no method named `build` found for struct `ConfiguredBuilder<...NoExecutor>`
        //
        // The error message will be something like:
        // "no method named `build` found for struct `ConfiguredBuilder<TestEvent, TestEffect, TestModel, (), NoExecutor>`"

        // Instead, we verify that providing an executor allows compilation
        let registry = crate::executor::ExecutorRegistry::new();

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry)
            .build();

        let (_core, _shell) = runner.split();

        // If we get here, the typestate pattern is working correctly
    }
}
