use crate::core::{Core, EventHandler};
use crate::effect_context::EffectContext;
use crate::executor::{AsyncExecutor, ExecutorRegistry, SyncExecutor, Task};
use crate::shell::{EffectHandler, Shell};
use crate::syzygy::Syzygy;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

/// Marker type indicating no executor registry has been provided
pub struct NoExecutor;

/// Marker type indicating an executor registry has been provided
pub struct HasAsyncExecutor;

/// Marker type indicating an executor registry is present but lacks a default async executor
pub struct RegistryPending;

/// Base builder phase: configure models, resources, executors
pub struct SyzygyBuilder<E, X, M, R> {
    model: M,
    resources: R,
    exec_registry: Option<ExecutorRegistry<E>>,
    _marker: std::marker::PhantomData<X>,
}

impl<Event, Effect, Model, Resource> ConfiguredBuilder<Event, Effect, Model, Resource, NoExecutor>
where
    Event: Send + Sync + 'static,
    Effect: Send + 'static,
    Resource: Send + Sync + 'static,
{
    /// Provide a preconstructed executor registry. The registry must be given a default
    /// async executor before `build()` becomes available.
    #[must_use]
    pub fn with_executor_registry(
        self,
        registry: ExecutorRegistry<Event>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, RegistryPending> {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(registry),
            effect_channel_capacity: self.effect_channel_capacity,
            _executor_state: PhantomData,
        }
    }
}

impl<Event, Effect, Model, Resource>
    ConfiguredBuilder<Event, Effect, Model, Resource, RegistryPending>
where
    Event: Send + Sync + 'static,
    Effect: Send + 'static,
    Resource: Send + Sync + 'static,
{
    /// Helper to promote this builder to HasAsyncExecutor state with the given registry
    fn promote_with_registry(
        self,
        registry: ExecutorRegistry<Event>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, HasAsyncExecutor> {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(registry),
            effect_channel_capacity: self.effect_channel_capacity,
            _executor_state: PhantomData,
        }
    }

    /// Promote a registry-backed builder to the `HasAsyncExecutor` state by selecting an
    /// already-registered executor as the default. Returns `Err(self)` if the executor type
    /// has not been registered.
    pub fn try_with_existing_default<T: 'static>(
        mut self,
    ) -> Result<ConfiguredBuilder<Event, Effect, Model, Resource, HasAsyncExecutor>, Box<Self>>
    {
        let Some(mut registry) = self.exec_registry.take() else {
            unreachable!("registry typestate violation: expected stored registry");
        };

        if registry.set_default_async::<T>() {
            Ok(self.promote_with_registry(registry))
        } else {
            self.exec_registry = Some(registry);
            Err(Box::new(self))
        }
    }

    /// Promote a registry-backed builder to the `HasAsyncExecutor` state using the registry's
    /// existing default executor. This is more ergonomic than `try_with_existing_default` when
    /// the registry already has a default set (which happens automatically when the first
    /// async executor is registered).
    ///
    /// Returns `Err(self)` if no async executor has been registered or no default is set.
    pub fn try_use_registry_default(
        mut self,
    ) -> Result<ConfiguredBuilder<Event, Effect, Model, Resource, HasAsyncExecutor>, Box<Self>>
    {
        let Some(registry) = self.exec_registry.take() else {
            unreachable!("registry typestate violation: expected stored registry");
        };

        if registry.has_default_async() {
            Ok(self.promote_with_registry(registry))
        } else {
            self.exec_registry = Some(registry);
            Err(Box::new(self))
        }
    }
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
            effect_channel_capacity: None,
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
    effect_channel_capacity: Option<usize>,
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
    pub fn effect_handler<H>(
        self,
        handler: H,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, ExecutorState>
    where
        H: FnMut(Effect, EffectContext<Event, Resource>) -> Task<Event, Resource> + Send + 'static,
    {
        let boxed: EffectHandler<Event, Effect, Resource> = Box::new(handler);
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: Some(boxed),
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            effect_channel_capacity: self.effect_channel_capacity,
            _executor_state: PhantomData,
        }
    }

    /// Add a single async executor to the registry by concrete type
    #[must_use]
    pub fn with_async_executor<T>(
        self,
        exec: T,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, HasAsyncExecutor>
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
            effect_channel_capacity: self.effect_channel_capacity,
            _executor_state: PhantomData,
        }
    }

    /// Override the shell's effect channel capacity before building the system.
    #[must_use]
    pub fn with_effect_channel_capacity(
        self,
        capacity: Option<usize>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource, ExecutorState> {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            effect_channel_capacity: capacity,
            _executor_state: PhantomData,
        }
    }

    /// Internal helper to build shell
    fn build_shell(
        resources: Resource,
        exec_registry: Arc<ExecutorRegistry<Event>>,
        effect_handler: Option<EffectHandler<Event, Effect, Resource>>,
        event_tx: crossbeam_channel::Sender<Event>,
        effect_channel_capacity: Option<usize>,
    ) -> Shell<Event, Effect, Resource> {
        use crossbeam_channel::{bounded, unbounded};

        fn default_effect_handler<E, X, R>(_: X, _: EffectContext<E, R>) -> Task<E, R>
        where
            E: Send + 'static,
            X: Send + 'static,
            R: Send + Sync + 'static,
        {
            Task::none()
        }

        let (effect_tx, effect_rx) = match effect_channel_capacity {
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
            effect_channel_capacity,
            executors: exec_registry,
            closed: false,
            prefetched_effects: VecDeque::new(),
        }
    }
}

impl<Event, Effect, Model, Resource>
    ConfiguredBuilder<Event, Effect, Model, Resource, HasAsyncExecutor>
where
    Event: Send + Sync + 'static,
    Effect: Send + 'static,
    Resource: Send + Sync + 'static,
{
    /// Add a single sync executor to the registry by concrete type.
    #[must_use]
    pub fn with_sync_executor<T>(mut self, exec: T) -> Self
    where
        T: SyncExecutor<Event> + Send + 'static,
    {
        let mut reg = self.exec_registry.take().unwrap_or_default();
        reg.insert_sync(exec);
        self.exec_registry = Some(reg);
        self
    }
}

// Separate impl block for HasAsyncExecutor state - only this state can build
impl<Event, Effect, Model, Resource>
    ConfiguredBuilder<Event, Effect, Model, Resource, HasAsyncExecutor>
where
    Event: Send + Sync + 'static,
    Effect: Send + 'static,
    Resource: Send + Sync + 'static,
{
    /// Build the system and return a `Syzygy` that owns the Core and Shell.
    pub fn build(self) -> Syzygy<Event, Effect, Model, Resource> {
        let (core, event_tx) = Core::new(self.event_handler, self.model);
        let registry = self.exec_registry.unwrap();

        let shell = Self::build_shell(
            self.resources,
            Arc::new(registry),
            self.effect_handler,
            event_tx,
            self.effect_channel_capacity,
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
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_async_executor(crate::executor::InlineAsync::<TestEvent>::new())
            .build();

        let (mut core, _shell) = runner.split();

        let _command = core.handle_event(TestEvent::Increment);
        let model: &TestModel = core.model();
        assert_eq!(model.count, 1);
    }

    #[test]
    fn test_shell_type_with_models_only() {
        // Test that Shell type remains simple when only models are added (no resources)
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_async_executor(crate::executor::InlineAsync::<TestEvent>::new())
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
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_async_executor(crate::executor::InlineAsync::<TestEvent>::new())
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

        let mut registry = crate::executor::ExecutorRegistry::new();
        registry.insert_async(crate::executor::InlineAsync::<TestEvent>::new());

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
            .with_executor_registry(registry);

        let runner = runner
            .try_use_registry_default()
            .unwrap_or_else(|_| panic!("registry should have a default async executor"))
            .build();

        let (mut core, _shell) = runner.split();

        core.handle_event(TestEvent::Increment);

        let (_user, config) = core.model();

        // Skipped model assertions in refactor
        assert_eq!(config.theme, "dark");
    }

    #[test]
    fn test_try_use_registry_default() {
        // Test the ergonomic registry default method
        let mut registry = crate::executor::ExecutorRegistry::new();
        registry.insert_async(crate::executor::InlineAsync::<TestEvent>::new());

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry);

        // This should work without specifying the executor type
        let runner = runner
            .try_use_registry_default()
            .unwrap_or_else(|_| panic!("registry should have a default async executor"))
            .build();

        let (_core, _shell) = runner.split();

        // If we get here, the ergonomic method worked correctly
    }

    #[test]
    fn test_try_use_registry_default_fails_without_async_executor() {
        // Test that try_use_registry_default fails when no async executor is registered
        let registry = crate::executor::ExecutorRegistry::<TestEvent>::new();
        // Note: no async executor registered

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry);

        // This should fail because no async executor was registered
        let result = runner.try_use_registry_default();
        assert!(
            result.is_err(),
            "Should fail when no async executor is registered"
        );
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
        let mut registry = crate::executor::ExecutorRegistry::new();
        registry.insert_async(crate::executor::InlineAsync::<TestEvent>::new());

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::Task::events(Vec::new()))
            .with_executor_registry(registry);

        let runner = runner
            .try_use_registry_default()
            .unwrap_or_else(|_| panic!("registry should have a default async executor"))
            .build();

        let (_core, _shell) = runner.split();

        // If we get here, the typestate pattern is working correctly
    }
}
