use crate::core::{Core, UpdateFn};
use crate::shell::Shell;
use crate::storage::{EmptyStorage, StorageBuilder};

/// Simple builder for creating Syzygy systems
pub struct SyzygyBuilder<Event, Effect, ModelStorage = EmptyStorage, ResourceStorage = EmptyStorage>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    update_fn: Option<UpdateFn<Event, Effect, ModelStorage>>,
    storage: ModelStorage,
    resources: ResourceStorage,
}

impl<Event, Effect> SyzygyBuilder<Event, Effect>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    /// Create a new builder with empty storage
    #[must_use]
    pub fn new() -> Self {
        Self {
            update_fn: None,
            storage: EmptyStorage,
            resources: EmptyStorage,
        }
    }
}

// Single implementation that works with all storage combinations
impl<Event, Effect, ModelStorage, ResourceStorage>
    SyzygyBuilder<Event, Effect, ModelStorage, ResourceStorage>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    ResourceStorage: Clone + Send + Sync + 'static,
{
    /// Add a model to the storage chain
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct CounterModel { count: i32 }
    /// # #[derive(Debug, Default)] struct UserModel { name: String }
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let builder = Syzygy::builder::<Event, Effect>()
    ///     .model(CounterModel::default())
    ///     .model(UserModel::default());
    /// ```
    #[must_use]
    pub fn model<M: 'static>(
        self,
        model: M,
    ) -> SyzygyBuilder<Event, Effect, <ModelStorage as StorageBuilder<M>>::Output, ResourceStorage>
    where
        ModelStorage: StorageBuilder<M>,
    {
        SyzygyBuilder {
            update_fn: None, // Type changed, need to set update_fn again
            storage: self.storage.with_model(model),
            resources: self.resources,
        }
    }

    /// Add a resource to the resource storage chain
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Clone)] struct Database;
    /// # #[derive(Clone)] struct ApiClient;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let builder = Syzygy::builder::<Event, Effect>()
    ///     .resource(Database)
    ///     .resource(ApiClient);
    /// ```
    #[must_use]
    pub fn resource<R: Send + Sync + 'static>(
        self,
        resource: R,
    ) -> SyzygyBuilder<Event, Effect, ModelStorage, <ResourceStorage as StorageBuilder<R>>::Output>
    where
        ResourceStorage: StorageBuilder<R>,
    {
        SyzygyBuilder {
            update_fn: self.update_fn,
            storage: self.storage,
            resources: self.resources.with_model(resource),
        }
    }

    /// Set the update function that processes events
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// fn my_update(event: Event, ctx: &mut EventContext<Event, Effect, Storage<Model, EmptyStorage>>) -> Command<Event, Effect> {
    ///     Command::none()
    /// }
    ///
    /// let builder = Syzygy::builder()
    ///     .model(Model::default())
    ///     .update(my_update);
    /// ```
    #[must_use]
    pub fn update(mut self, update_fn: UpdateFn<Event, Effect, ModelStorage>) -> Self {
        self.update_fn = Some(update_fn);
        self
    }

    /// Build the system with auto-wired Shell connected to Core's event channel
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .update(|_event: Event, _ctx| Command::none())
    ///     .build();
    /// ```
    pub fn build(
        self,
    ) -> (
        Core<Event, Effect, ModelStorage>,
        Shell<Event, Effect, ResourceStorage>,
    ) {
        let update_fn = self
            .update_fn
            .expect("Update function must be provided before building");

        let (core, event_tx) = Core::new(update_fn, self.storage);
        let shell = Self::build_shell_with_resources_and_event_tx(self.resources, Some(event_tx));

        (core, shell)
    }

    /// Build the system with manual wiring
    pub fn build_manual(
        self,
    ) -> (
        Core<Event, Effect, ModelStorage>,
        Shell<Event, Effect, ResourceStorage>,
    ) {
        let update_fn = self
            .update_fn
            .expect("Update function must be provided before building");

        let (core, _event_tx) = Core::new(update_fn, self.storage);
        let shell = Self::build_shell_with_resources_and_event_tx(self.resources, None);

        (core, shell)
    }

    /// Internal helper to build shell with resources and optional event sender
    fn build_shell_with_resources_and_event_tx(
        resources: ResourceStorage,
        event_tx: Option<crossbeam_channel::Sender<Event>>,
    ) -> Shell<Event, Effect, ResourceStorage> {
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
            event_tx,
            resources,
            effect_handler: (),
            config,
        }
    }
}

impl<Event, Effect> Default for SyzygyBuilder<Event, Effect>
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Main entry point for creating Syzygy systems
pub struct Syzygy;

impl Syzygy {
    /// Create a new builder for the given Event and Effect types
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .update(|_event: Event, _ctx| Command::none())
    ///     .build();
    /// ```
    #[must_use]
    pub fn builder<Event, Effect>() -> SyzygyBuilder<Event, Effect>
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
            .update(test_update)
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
            .update(multi_update)
            .build();

        core.handle_event(TestEvent::Increment);

        let user: &UserModel = core.storage().get();
        let config: &ConfigModel = core.storage().get();

        assert_eq!(user.name, "Updated");
        assert_eq!(config.theme, "dark");
    }
}
