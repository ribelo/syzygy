use crate::core::{Core, EventHandler};
use crate::prelude::EffectHandler;
use crate::shell::Shell;
use crate::storage::{EmptyStorage, StorageBuilder};

/// Base builder phase: configure models, resources, executors
pub struct SyzygyBuilder<
    Event,
    Effect,
    ModelStorage = EmptyStorage,
    ResourceStorage = EmptyStorage,
    ExecutorStorage = crate::executor::EmptyExecutorStorage,
> {
    storage: ModelStorage,
    resources: ResourceStorage,
    executors: ExecutorStorage,
    _marker: std::marker::PhantomData<(Event, Effect)>,
}

impl<Event, Effect>
    SyzygyBuilder<Event, Effect, EmptyStorage, EmptyStorage, crate::executor::EmptyExecutorStorage>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    /// Create a new builder with empty storage
    #[must_use]
    pub fn new() -> Self {
        Self {
            storage: EmptyStorage::new(),
            resources: EmptyStorage::new(),
            executors: crate::executor::EmptyExecutorStorage::default(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Event, Effect, ModelStorage, ResourceStorage, ExecutorStorage>
    SyzygyBuilder<Event, Effect, ModelStorage, ResourceStorage, ExecutorStorage>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    ResourceStorage: Clone + Send + Sync + 'static,
    ExecutorStorage: Clone + Send + Sync + 'static,
{
    /// Add a model to the storage chain
    #[must_use]
    pub fn model<M: 'static>(
        self,
        model: M,
    ) -> SyzygyBuilder<Event, Effect, <ModelStorage as StorageBuilder<M>>::Output, ResourceStorage, ExecutorStorage>
    where
        ModelStorage: StorageBuilder<M>,
    {
        SyzygyBuilder {
            storage: self.storage.with_model(model),
            resources: self.resources,
            executors: self.executors,
            _marker: std::marker::PhantomData,
        }
    }

    /// Add a resource to the resource storage chain
    #[must_use]
    pub fn resource<R: Send + Sync + 'static>(
        self,
        resource: R,
    ) -> SyzygyBuilder<Event, Effect, ModelStorage, <ResourceStorage as StorageBuilder<R>>::Output, ExecutorStorage>
    where
        ResourceStorage: StorageBuilder<R>,
    {
        SyzygyBuilder {
            storage: self.storage,
            resources: self.resources.with_model(resource),
            executors: self.executors,
            _marker: std::marker::PhantomData,
        }
    }

    /// Add an executor to the executor storage chain
    #[must_use]
    pub fn executor<E: Send + Sync + 'static>(
        self,
        executor: E,
    ) -> SyzygyBuilder<Event, Effect, ModelStorage, ResourceStorage, <ExecutorStorage as crate::executor::storage::StorageBuilder<E>>::Output>
    where
        ExecutorStorage: crate::executor::storage::StorageBuilder<E>,
    {
        SyzygyBuilder {
            storage: self.storage,
            resources: self.resources,
            executors: self.executors.with_executor(executor),
            _marker: std::marker::PhantomData,
        }
    }

    /// Transition to configured phase by setting the event handler
    #[must_use]
    pub fn event_handler(
        self,
        event_handler: EventHandler<Event, Effect, ModelStorage>,
    ) -> ConfiguredBuilder<Event, Effect, ModelStorage, ResourceStorage, ExecutorStorage, ()> {
        ConfiguredBuilder {
            event_handler,
            effect_handler: None,
            storage: self.storage,
            resources: self.resources,
            executors: self.executors,
        }
    }
}

/// Configured phase: handlers are set; building is allowed. No more storage changes to avoid type surprises.
pub struct ConfiguredBuilder<
    Event,
    Effect,
    ModelStorage,
    ResourceStorage,
    ExecutorStorage,
    H = (),
> {
    event_handler: EventHandler<Event, Effect, ModelStorage>,
    effect_handler: Option<H>,
    storage: ModelStorage,
    resources: ResourceStorage,
    executors: ExecutorStorage,
}

impl<Event, Effect, ModelStorage, ResourceStorage, ExecutorStorage, H>
    ConfiguredBuilder<Event, Effect, ModelStorage, ResourceStorage, ExecutorStorage, H>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    ResourceStorage: Clone + Send + Sync + 'static,
    ExecutorStorage: Clone + Send + Sync + 'static,
{
    /// Set the effect handler that processes effects
    #[must_use]
    pub fn effect_handler<H2>(self, handler: H2) -> ConfiguredBuilder<Event, Effect, ModelStorage, ResourceStorage, ExecutorStorage, H2>
    where
        H2: EffectHandler<Event, Effect, ResourceStorage, ExecutorStorage> + 'static,
    {
        ConfiguredBuilder {
            event_handler: self.event_handler,
            effect_handler: Some(handler),
            storage: self.storage,
            resources: self.resources,
            executors: self.executors,
        }
    }

    /// Build the system with auto-wired Shell connected to Core's event channel
    pub fn build(
        self,
    ) -> (
        Core<Event, Effect, ModelStorage>,
        Shell<Event, Effect, ResourceStorage, ExecutorStorage, H>,
    )
    where
        H: EffectHandler<Event, Effect, ResourceStorage, ExecutorStorage> + 'static,
    {
        let (core, event_tx) = Core::new(self.event_handler, self.storage);
        let shell = Self::build_shell(self.resources, self.executors, self.effect_handler, event_tx);
        (core, shell)
    }

    /// Build the system with manual wiring
    pub fn build_manual(
        self,
    ) -> (
        Core<Event, Effect, ModelStorage>,
        Shell<Event, Effect, ResourceStorage, ExecutorStorage, H>,
    )
    where
        H: EffectHandler<Event, Effect, ResourceStorage, ExecutorStorage> + 'static,
    {
        let (core, event_tx) = Core::new(self.event_handler, self.storage);
        let shell = Self::build_shell(self.resources, self.executors, self.effect_handler, event_tx);
        (core, shell)
    }

    /// Internal helper to build shell
    fn build_shell(
        resources: ResourceStorage,
        executors: ExecutorStorage,
        effect_handler: Option<H>,
        event_tx: crossbeam_channel::Sender<Event>,
    ) -> Shell<Event, Effect, ResourceStorage, ExecutorStorage, H>
    where
        H: EffectHandler<Event, Effect, ResourceStorage, ExecutorStorage> + 'static,
    {
        use crate::shell::ShellConfig;
        use crate::task::TaskTracker;
        use crossbeam_channel::unbounded;
        use std::sync::{Arc, Mutex};

        let config = ShellConfig::default();
        let (effect_tx, effect_rx) = unbounded();

        Shell {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            effect_rx,
            effect_tx,
            event_tx: Some(event_tx),
            resources,
            effect_handler: effect_handler
                .unwrap_or_else(|| panic!("Effect handler must be provided before building")),
            config,
            executors,
        }
    }
}

/// Main entry point for creating Syzygy systems
pub struct Syzygy;

impl Syzygy {
    /// Create a new builder for the given Event and Effect types
    #[must_use]
    pub fn builder<Event, Effect>() -> SyzygyBuilder<Event, Effect, EmptyStorage, EmptyStorage, crate::executor::EmptyExecutorStorage>
    where
        Event: Clone + Send + 'static,
        Effect: Clone + Send + 'static,
    {
        SyzygyBuilder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::storage::{EmptyStorage, Storage};

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
        ctx: &mut crate::event_context::EventContext<
            TestEvent,
            TestEffect,
            Storage<TestModel, EmptyStorage>,
        >,
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
            .effect_handler(|_e: TestEffect, _ctx| async { crate::streaming::EffectOutput::None })
            .build();

        let _command = core.handle_event(TestEvent::Increment);
        let model: &TestModel = core.storage().get();
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
                Storage<ConfigModel, Storage<UserModel, EmptyStorage>>,
            >,
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment => {
                    {
                        let user: &mut UserModel = ctx.model_mut();
                        user.name = "Updated".to_string();
                    }
                    {
                        let config: &mut ConfigModel = ctx.model_mut();
                        config.theme = "dark".to_string();
                    }

                    Command::effect(TestEffect::Log)
                }
            }
        }

        let (mut core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(UserModel {
                name: "Alice".to_string(),
            })
            .model(ConfigModel {
                theme: "light".to_string(),
            })
            .event_handler(multi_update)
            .effect_handler(|_e: TestEffect, _ctx| async { crate::streaming::EffectOutput::None })
            .build();

        core.handle_event(TestEvent::Increment);

        let user: &UserModel = core.storage().get();
        let config: &ConfigModel = core.storage().get();

        assert_eq!(user.name, "Updated");
        assert_eq!(config.theme, "dark");
    }
}
