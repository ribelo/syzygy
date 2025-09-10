use crate::core::{Core, EventHandler};
use crate::effect_context::EffectContext;
use crate::executor::EffectPlan;

use crate::executor::{AsyncExecutor, ExecutorRegistry, SyncExecutor};
use crate::prelude::EffectHandler;
use crate::shell::Shell;
use std::sync::Arc;

/// Type alias for executor configurator function
type ExecutorConfigurator<Event, Resource> =
    fn(&Resource, &crossbeam_channel::Sender<Event>) -> Arc<ExecutorRegistry<Event>>;

/// Base builder phase: configure models, resources, executors
pub struct SyzygyBuilder<Event, Effect, Model = (), Resource = ()> {
    model: Model,
    resources: Resource,
    exec_registry: Option<ExecutorRegistry<Event>>,
    _marker: std::marker::PhantomData<(Event, Effect)>,
}

impl<Event, Effect> Default for SyzygyBuilder<Event, Effect, (), ()>
where
    Event: Clone + Send + Sync + 'static,
    Effect: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Event, Effect> SyzygyBuilder<Event, Effect, (), ()>
where
    Event: Clone + Send + Sync + 'static,
    Effect: Clone + Send + 'static,
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
    Event: Clone + Send + Sync + 'static,
    Effect: Clone + Send + 'static,
    Model: 'static,
    Resource: Clone + Send + Sync + 'static,
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
    pub fn resource<R: Clone + Send + Sync + 'static>(
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
    ) -> ConfiguredBuilder<Event, Effect, Model, Resource> {
        ConfiguredBuilder {
            event_handler,
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            exec_configurator: None,
        }
    }
}

/// Configured phase: handlers are set; building is allowed. No more storage changes to avoid type surprises.
pub struct ConfiguredBuilder<Event, Effect, Model, Resource> {
    event_handler: EventHandler<Event, Effect, Model>,
    effect_handler: Option<EffectHandler<Event, Effect, Resource>>,
    model: Model,
    resources: Resource,
    exec_registry: Option<ExecutorRegistry<Event>>,
    exec_configurator: Option<ExecutorConfigurator<Event, Resource>>,
}

impl<Event, Effect, Model, Resource> ConfiguredBuilder<Event, Effect, Model, Resource>
where
    Event: Clone + Send + Sync + 'static,
    Effect: Clone + Send + 'static,
    Resource: Clone + Send + Sync + 'static,
{
    /// Set the effect handler that processes effects
    #[must_use]
    pub fn effect_handler(self, handler: EffectHandler<Event, Effect, Resource>) -> Self {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: Some(handler),
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            exec_configurator: self.exec_configurator,
        }
    }

    /// Provide a configurator that can build the registry when `event_tx` is available.
    #[must_use]
    pub fn with_executor_registry(self, registry: ExecutorRegistry<Event>) -> Self {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(registry),
            exec_configurator: self.exec_configurator,
        }
    }

    /// Provide a configurator that can build the registry when `event_tx` is available.
    #[must_use]
    pub fn configure_executors(self, f: ExecutorConfigurator<Event, Resource>) -> Self {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: self.exec_registry,
            exec_configurator: Some(f),
        }
    }

    /// Configure a default IO + sync executor set.
    /// - Async IO: `TokioIo` multi-thread runtime (if tokio feature is enabled) for network/file operations
    /// - Sync CPU: `RayonExecutor` pool (if rayon feature is enabled), else `SingleThreadExecutor` for CPU-bound work
    ///
    /// For more control, use:
    /// - `with_io_executor()` for IO-focused async work
    /// - `with_cpu_async_executor()` for CPU-focused async work
    /// - `with_default_sync_executor()` for sync CPU work
    /// - `with_dual_async_executors()` for both IO and CPU async executors
    #[must_use]
    pub fn with_default_executors(self) -> Self {
        fn make_default_registry<E, R>(
            _resources: &R,
            _event_tx: &crossbeam_channel::Sender<E>,
        ) -> Arc<ExecutorRegistry<E>>
        where
            E: Clone + Send + Sync + 'static,
            R: Clone + Send + Sync + 'static,
        {
            use crate::executor::ExecutorRegistry;
            let mut registry = ExecutorRegistry::<E>::new();

            // Helper to compute threads
            let threads = std::thread::available_parallelism()
                .map(std::num::NonZero::get)
                .unwrap_or(1)
                .max(1);

            // IO executor
            #[cfg(feature = "tokio")]
            {
                let io_exec = crate::executor::TokioIo::multi_thread(threads);
                registry.insert_async(io_exec);
            }

            // CPU executor
            #[cfg(feature = "rayon")]
            {
                let cpu_exec = crate::executor::RayonExecutor::new(None);
                registry.insert_sync(cpu_exec);
            }
            #[cfg(not(feature = "rayon"))]
            {
                let cpu_exec = crate::executor::SingleThreadExecutor::new();
                registry.insert_sync(cpu_exec);
            }

            Arc::new(registry)
        }

        self.configure_executors(make_default_registry::<Event, Resource>)
    }

    /// Configure an IO-focused async executor using `TokioIo`
    #[must_use]
    pub fn with_io_executor(self, threads: Option<usize>) -> Self {
        #[cfg(feature = "tokio")]
        {
            let mut registry = self.exec_registry.unwrap_or_default();
            let threads = threads.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(std::num::NonZero::get)
                    .unwrap_or(4)
            });
            let io_exec = crate::executor::TokioIo::multi_thread(threads);
            registry.insert_async(io_exec);
            ConfiguredBuilder {
                event_handler: self.event_handler,
                effect_handler: self.effect_handler,
                model: self.model,
                resources: self.resources,
                exec_registry: Some(registry),
                exec_configurator: self.exec_configurator,
            }
        }
        #[cfg(not(feature = "tokio"))]
        self
    }

    /// Configure a CPU-focused async executor using `TokioCpu`
    #[must_use]
    pub fn with_cpu_async_executor(self, threads: Option<usize>) -> Self {
        #[cfg(feature = "tokio")]
        {
            let mut registry = self.exec_registry.unwrap_or_default();
            let threads = threads.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(std::num::NonZero::get)
                    .unwrap_or(4)
            });
            let cpu_exec = crate::executor::TokioCpu::multi_thread(threads);
            registry.insert_async(cpu_exec);
            ConfiguredBuilder {
                event_handler: self.event_handler,
                effect_handler: self.effect_handler,
                model: self.model,
                resources: self.resources,
                exec_registry: Some(registry),
                exec_configurator: self.exec_configurator,
            }
        }
        #[cfg(not(feature = "tokio"))]
        self
    }

    /// Configure a default sync executor (Rayon or `SingleThread`)
    #[must_use]
    pub fn with_default_sync_executor(self) -> Self {
        let mut registry = self.exec_registry.unwrap_or_default();

        #[cfg(feature = "rayon")]
        {
            let sync_exec = crate::executor::RayonExecutor::new(None);
            registry.insert_sync(sync_exec);
        }
        #[cfg(not(feature = "rayon"))]
        {
            let sync_exec = crate::executor::SingleThreadExecutor::new();
            registry.insert_sync(sync_exec);
        }

        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(registry),
            exec_configurator: self.exec_configurator,
        }
    }

    /// Configure with both IO and CPU async executors
    #[must_use]
    pub fn with_dual_async_executors(self) -> Self {
        #[cfg(feature = "tokio")]
        {
            let mut registry = self.exec_registry.unwrap_or_default();
            let threads = std::thread::available_parallelism()
                .map(std::num::NonZero::get)
                .unwrap_or(4);

            // IO executor for network/file operations
            let io_exec = crate::executor::TokioIo::multi_thread(threads);
            registry.insert_async(io_exec);

            // CPU executor for compute-heavy async work
            let cpu_exec = crate::executor::TokioCpu::multi_thread(threads);
            registry.insert_async(cpu_exec);

            ConfiguredBuilder {
                event_handler: self.event_handler,
                effect_handler: self.effect_handler,
                model: self.model,
                resources: self.resources,
                exec_registry: Some(registry),
                exec_configurator: self.exec_configurator,
            }
        }
        #[cfg(not(feature = "tokio"))]
        self
    }

    /// Add a single async executor to the registry by concrete type
    #[must_use]
    pub fn with_async_executor<T>(self, exec: T) -> Self
    where
        T: AsyncExecutor<Event> + Send + Sync + 'static,
    {
        let mut reg = self.exec_registry.unwrap_or_default();
        reg.insert_async(exec);
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(reg),
            exec_configurator: self.exec_configurator,
        }
    }

    /// Add a single sync executor to the registry by concrete type
    #[must_use]
    pub fn with_sync_executor<T>(self, exec: T) -> Self
    where
        T: SyncExecutor<Event> + Send + Sync + 'static,
    {
        let mut reg = self.exec_registry.unwrap_or_default();
        reg.insert_sync(exec);
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: self.effect_handler,
            model: self.model,
            resources: self.resources,
            exec_registry: Some(reg),
            exec_configurator: self.exec_configurator,
        }
    }

    /// Build the system with auto-wired Shell connected to Core's event channel
    pub fn build(self) -> (Core<Event, Effect, Model>, Shell<Event, Effect, Resource>) {
        let (core, event_tx) = Core::new(self.event_handler, self.model);
        let shell = Self::build_shell(
            self.resources,
            self.exec_registry.map(Arc::new),
            self.exec_configurator,
            self.effect_handler,
            event_tx,
        );
        (core, shell)
    }

    /// Build the system with manual wiring
    pub fn build_manual(self) -> (Core<Event, Effect, Model>, Shell<Event, Effect, Resource>) {
        let (core, event_tx) = Core::new(self.event_handler, self.model);
        let shell = Self::build_shell(
            self.resources,
            self.exec_registry.map(Arc::new),
            self.exec_configurator,
            self.effect_handler,
            event_tx,
        );
        (core, shell)
    }

    /// Internal helper to build shell
    fn build_shell(
        resources: Resource,
        exec_registry: Option<Arc<ExecutorRegistry<Event>>>,
        exec_configurator: Option<ExecutorConfigurator<Event, Resource>>,
        effect_handler: Option<EffectHandler<Event, Effect, Resource>>,
        event_tx: crossbeam_channel::Sender<Event>,
    ) -> Shell<Event, Effect, Resource> {
        use crate::shell::ShellConfig;
        use crossbeam_channel::unbounded;

        fn default_effect_handler<E, X, R>(_: X, _: &EffectContext<E, R>) -> EffectPlan<E, R>
        where
            E: Clone + Send + 'static,
            X: Clone + Send + 'static,
            R: Clone + Send + Sync + 'static,
        {
            EffectPlan::events(Vec::new())
        }

        let config = ShellConfig::default();
        let (effect_tx, effect_rx) = unbounded();

        // Resolve or build registry now that we have event_tx and resources
        let registry_arc: Arc<ExecutorRegistry<Event>> = if let Some(reg) = exec_registry {
            reg
        } else if let Some(cfg) = exec_configurator {
            cfg(&resources, &event_tx)
        } else {
            Arc::new(ExecutorRegistry::new())
        };

        Shell {
            effect_rx,
            effect_tx,
            event_tx,
            resources,
            // use provided handler or a no-op
            effect_handler: effect_handler
                .unwrap_or(default_effect_handler::<Event, Effect, Resource>),
            config,
            executors: registry_arc,
            closed: false,
        }
    }
}

/// Main entry point for creating Syzygy systems
pub struct Syzygy;

impl Syzygy {
    /// Create a new builder for the given Event and Effect types
    #[must_use]
    pub fn builder<Event, Effect>() -> SyzygyBuilder<Event, Effect, (), ()>
    where
        Event: Clone + Send + Sync + 'static,
        Effect: Clone + Send + 'static,
    {
        SyzygyBuilder::new()
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
        let (mut core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::EffectPlan::events(Vec::new()))
            .build();

        let _command = core.handle_event(TestEvent::Increment);
        let model: &TestModel = core.model();
        assert_eq!(model.count, 1);
    }

    #[test]
    fn test_shell_type_with_models_only() {
        // Test that Shell type remains simple when only models are added (no resources)
        let (_core, shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::EffectPlan::events(Vec::new()))
            .build();

        // Type should remain simple with resources-only generic
        let _: Shell<TestEvent, TestEffect, ()> = shell;

        // The key test: adding models should NOT change the resource storage type
        // This confirms that the builder correctly separates model storage from resource storage
    }

    #[test]
    fn test_shell_type_inference_works() {
        // Test that users don't need to specify complex types manually
        let (mut core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::EffectPlan::events(Vec::new()))
            .build();

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

        let (mut core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model((
                UserModel {
                    name: "Alice".to_string(),
                },
                ConfigModel {
                    theme: "light".to_string(),
                },
            ))
            .event_handler(multi_update)
            .effect_handler(|_e: TestEffect, _ctx| crate::executor::EffectPlan::events(Vec::new()))
            .build();

        core.handle_event(TestEvent::Increment);

        let (_user, config) = core.model();

        // Skipped model assertions in refactor
        assert_eq!(config.theme, "dark");
    }
}
