use crate::core::{Core, UpdateFn};
use crate::shell::Shell;
use crate::storage::{EmptyStorage, Storage};

/// Simple builder for creating Syzygy systems
///
/// This builder is generic over Storage types, allowing you to add models
/// at compile time with full type safety. Start with an empty builder and
/// chain .model() calls to build up your storage.
pub struct SyzygyBuilder<Event, Effect, ModelStorage = EmptyStorage, ResourceStorage = EmptyStorage> 
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    update_fn: Option<UpdateFn<Event, Effect, ModelStorage>>,
    storage: ModelStorage,
    resources: ResourceStorage,
}

impl<Event, Effect> SyzygyBuilder<Event, Effect, EmptyStorage, EmptyStorage> 
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

// No general impl block needed - resources are added through specialized impls like models

// Specialized impl for adding models to EmptyStorage
impl<Event, Effect, ResourceStorage> SyzygyBuilder<Event, Effect, EmptyStorage, ResourceStorage> 
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    /// Add a model to empty storage
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, Storage<M, EmptyStorage>, ResourceStorage> {
        SyzygyBuilder {
            update_fn: None, // Type changed, need to set update_fn again
            storage: self.storage.with_model(model),
            resources: self.resources,
        }
    }
}

// Specialized impl for adding models to Storage chains
impl<Event, Effect, Head, Tail, ResourceStorage> SyzygyBuilder<Event, Effect, Storage<Head, Tail>, ResourceStorage> 
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Storage<Head, Tail>: crate::storage::RuntimeContains,
{
    /// Add a model to existing storage chain
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, Storage<M, Storage<Head, Tail>>, ResourceStorage> {
        SyzygyBuilder {
            update_fn: None, // Type changed, need to set update_fn again
            storage: self.storage.with_model(model),
            resources: self.resources,
        }
    }
}

// Specialized impl for adding resources to EmptyStorage
impl<Event, Effect, ModelStorage> SyzygyBuilder<Event, Effect, ModelStorage, EmptyStorage> 
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
{
    /// Add a resource to empty resource storage
    #[must_use]
    pub fn resource<R: Send + Sync + 'static>(self, resource: R) -> SyzygyBuilder<Event, Effect, ModelStorage, Storage<R, EmptyStorage>> {
        SyzygyBuilder {
            update_fn: self.update_fn,
            storage: self.storage,
            resources: self.resources.with_model(resource),
        }
    }
}

// Specialized impl for adding resources to Storage chains
impl<Event, Effect, ModelStorage, Head, Tail> SyzygyBuilder<Event, Effect, ModelStorage, Storage<Head, Tail>> 
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    Storage<Head, Tail>: crate::storage::RuntimeContains,
{
    /// Add a resource to existing resource storage chain
    #[must_use]
    pub fn resource<R: Send + Sync + 'static>(self, resource: R) -> SyzygyBuilder<Event, Effect, ModelStorage, Storage<R, Storage<Head, Tail>>> {
        SyzygyBuilder {
            update_fn: self.update_fn,
            storage: self.storage,
            resources: self.resources.with_model(resource),
        }
    }
}

impl<Event, Effect, ModelStorage, ResourceStorage> SyzygyBuilder<Event, Effect, ModelStorage, ResourceStorage> 
where
    Event: Clone + Send + 'static,
    Effect: Clone + Send + 'static,
    ResourceStorage: Send + Sync + 'static,
{
    /// Set the update function that processes events
    /// 
    /// The update function takes an event and mutable storage access,
    /// returning a Command describing effects to execute.
    #[must_use]
    pub fn update(mut self, update_fn: UpdateFn<Event, Effect, ModelStorage>) -> Self {
        self.update_fn = Some(update_fn);
        self
    }

    /// Build the system with auto-wired Shell connected to Core's event channel
    ///
    /// This is the default and recommended way to build a Syzygy system.
    /// The Shell is automatically connected to Core so effects can send events back.
    /// Update function must be provided before calling build().
    pub fn build(self) -> (Core<Event, Effect, ModelStorage>, Shell<Event, Effect, ResourceStorage>) {
        let update_fn = self.update_fn.expect("Update function must be provided before building");

        let SyzygyBuilder { storage, resources, .. } = self;
        let (core, event_tx) = Core::new(update_fn, storage);
        let shell = Self::build_shell_with_resources_and_event_tx(resources, Some(event_tx));

        (core, shell)
    }

    /// Build the system with manual wiring
    ///
    /// This gives maximum flexibility by keeping Core and Shell independent.
    /// You must manually connect the Shell to Core's event channel if you want
    /// effects to send events back for processing.
    pub fn build_manual(self) -> (Core<Event, Effect, ModelStorage>, Shell<Event, Effect, ResourceStorage>) {
        let update_fn = self.update_fn.expect("Update function must be provided before building");

        let SyzygyBuilder { storage, resources, .. } = self;
        let (core, _event_tx) = Core::new(update_fn, storage);
        let shell = Self::build_shell_with_resources_and_event_tx(resources, None);

        (core, shell)
    }
    
    /// Internal helper to build shell with resources and optional event sender
    fn build_shell_with_resources_and_event_tx(resources: ResourceStorage, event_tx: Option<crossbeam_channel::Sender<Event>>) -> Shell<Event, Effect, ResourceStorage> {
        use crossbeam_channel::unbounded;
        use std::sync::{Arc, Mutex};
        use crate::task::TaskTracker;
        use crate::shell::ShellConfig;
        
        let config = ShellConfig::default();
        let (effect_tx, effect_rx) = unbounded();
        
        Shell {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            effect_rx,
            effect_tx,
            event_tx,
            resources: Arc::new(resources),
            effect_handler: (),
            config,
        }
    }
}

impl<Event, Effect> Default for SyzygyBuilder<Event, Effect, EmptyStorage, EmptyStorage> 
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
    #[must_use] 
    pub fn builder<Event, Effect>() -> SyzygyBuilder<Event, Effect, EmptyStorage, EmptyStorage> 
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
        ctx: &mut crate::event_context::EventContext<TestEvent, TestEffect, Storage<TestModel, EmptyStorage>>
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

        // Test that we can handle events through the core
        let _command = core.handle_event(TestEvent::Increment);
        let model: &TestModel = core.storage().get();
        assert_eq!(model.count, 1);

        // Command should contain an effect
        // (We can't easily test this without executing the command stream)
    }

    #[test]
    fn test_auto_wiring() {
        let (core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .update(test_update)
            .build(); // Default is now auto-wired

        // Verify that Shell has been wired to Core's event channel
        let _event_sender = core.event_sender();

        // The shell should be able to receive events sent through this channel
        // (This is hard to test without actually running the async machinery,
        //  but we can at least verify the setup doesn't panic)
    }

    #[test]
    fn test_manual_wiring() {
        let (core, _shell) = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .update(test_update)
            .build_manual(); // Explicit manual wiring

        let _event_sender = core.event_sender();
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
            ctx: &mut crate::event_context::EventContext<TestEvent, TestEffect, Storage<ConfigModel, Storage<UserModel, EmptyStorage>>>
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
            .model(UserModel { name: "Alice".to_string() })
            .model(ConfigModel { theme: "light".to_string() })
            .update(multi_update)
            .build();

        // Test multi-model access
        core.handle_event(TestEvent::Increment);
        
        let user: &UserModel = core.storage().get();
        let config: &ConfigModel = core.storage().get();
        
        assert_eq!(user.name, "Updated");
        assert_eq!(config.theme, "dark");
    }
}