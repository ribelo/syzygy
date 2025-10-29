//! Builder for composing Core, Shell, resources, and executors.
//!
//! The builder wires your pure event handler to the async effect handler and
//! registers any executors you want the Shell to use. Keep models and resources
//! cheap to move/clone; the Shell clones resources for each effect call.
//!
//! This module intentionally avoids traits and lifetimes in the public surface
//! so usage stays straightforward in real apps and tests.
use crate::core::{Core, EventHandler};
use crate::executor::{
    AsyncExecutor, BlockingExecutor, ExecutorRegistry, ResourceBlockingExecutor, Task,
};
use crate::shell::{EffectHandler, Shell};
use crate::syzygy::Syzygy;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

/// Entry point for building a `Syzygy`.
///
/// Typical flow:
/// - set `.model(..)` and optional `.with_resources(..)`
/// - install `.event_handler(..)` and `.effect_handler(..)`
/// - register executors via `.with_*_executor(..)`
/// - call `.build()`
pub struct SyzygyBuilder<E, X, M, R = ()>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
    R: Clone + Send + Sync + 'static,
{
    model: M,
    resources: R,
    _marker: PhantomData<(E, X)>,
}

impl<E, X> Default for SyzygyBuilder<E, X, (), ()>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, X> SyzygyBuilder<E, X, (), ()>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: (),
            resources: (),
            _marker: PhantomData,
        }
    }
}

impl<Event, Effect, Model, Resources> SyzygyBuilder<Event, Effect, Model, Resources>
where
    Event: Send + Sync + 'static,
    Effect: Send + Sync + 'static,
    Model: 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Replace the current model with a new one.
    ///
    /// Models are owned by `Core` and mutated only by your event handler.
    /// Calling `.model(..)` more than once replaces the previous value—compose
    /// your application state inside a single struct or tuple if you need
    /// multiple parts of state.
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M, Resources> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            _marker: PhantomData,
        }
    }

    /// Install application resources that all effects can access.
    ///
    /// The Shell clones `Resources` per effect call. Use `Arc<_>` for heavy
    /// dependencies (DB pools, clients) or wrap interior mutability explicitly.
    #[must_use]
    pub fn with_resources<R2>(self, resources: R2) -> SyzygyBuilder<Event, Effect, Model, R2>
    where
        R2: Clone + Send + Sync + 'static,
    {
        SyzygyBuilder {
            model: self.model,
            resources,
            _marker: PhantomData,
        }
    }

    /// Finalize model configuration and set the event handler.
    ///
    /// Your event handler is a pure function. It mutates the model and returns
    /// a `Command` telling the Shell what effects to run.
    #[must_use]
    pub fn event_handler(
        self,
        event_handler: EventHandler<Event, Effect, Model>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resources> {
        ConfiguredBuilder {
            event_handler,
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            exec_registry: ExecutorRegistry::default(),
            effect_channel_capacity: None,
            _marker: PhantomData,
        }
    }
}

/// Builder stage where handlers and executors are configured.
///
/// After installing handlers you can register any number of executors. If a
/// `Task` targets a specific executor type you didn’t register, the Shell will
/// error when scheduling it. You can avoid registry lookups entirely by using
/// `Task::async_current`/`Task::stream_current` (runs on the current Tokio
/// runtime if available; otherwise completes inline by blocking the thread).
pub struct ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: Send + Sync + 'static,
    Effect: Send + Sync + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    event_handler: EventHandler<Event, Effect, Model>,
    effect_handler: Option<EffectHandler<Event, Effect, Resources>>,
    model: Model,
    resources: Resources,
    exec_registry: ExecutorRegistry<Event>,
    effect_channel_capacity: Option<usize>,
    _marker: PhantomData<Effect>,
}

impl<Event, Effect, Model, Resources> ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: Send + Sync + 'static,
    Effect: Send + Sync + 'static,
    Model: 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Install the effect handler.
    ///
    /// The handler receives a single effect and the cloned `Resources` value
    /// and returns a `Task` plan. The Shell drives the plan on the registered
    /// executors.
    #[must_use]
    pub fn effect_handler<H>(mut self, handler: H) -> Self
    where
        H: FnMut(Effect, Resources) -> Task<Event, Effect> + Send + 'static,
    {
        self.effect_handler = Some(Box::new(handler));
        self
    }

    /// Replace the executor registry with a pre-built one.
    #[must_use]
    pub fn with_executor_registry(mut self, registry: ExecutorRegistry<Event>) -> Self {
        self.exec_registry = registry;
        self
    }

    /// Register an async executor.
    ///
    /// Use for futures/streams scheduling (e.g. `TokioExecutor`, `InlineAsync`).
    #[must_use]
    pub fn with_async_executor<T>(mut self, exec: T) -> Self
    where
        T: AsyncExecutor<Event> + Send + Sync + 'static,
    {
        self.exec_registry.insert_async(exec);
        self
    }

    /// Register a blocking executor.
    ///
    /// Use for CPU-bound or blocking work that doesn’t require a shared
    /// resource (e.g. Rayon-based executor).
    #[must_use]
    pub fn with_blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: BlockingExecutor<Event> + Send + Sync + 'static,
    {
        self.exec_registry.insert_blocking(exec);
        self
    }

    /// Register a resource-blocking executor (single-threaded shared resource).
    ///
    /// Use when jobs need mutable access to a single owned resource with FIFO
    /// guarantees (e.g. a device handle that must not be used concurrently).
    #[must_use]
    pub fn with_resource_blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: ResourceBlockingExecutor<Event> + Send + Sync + 'static,
    {
        self.exec_registry.insert_resource_blocking(exec);
        self
    }

    /// Override the shell effect channel capacity.
    ///
    /// `None` means unbounded. Bounded channels apply backpressure to the
    /// caller when the effect queue is saturated.
    #[must_use]
    pub fn with_effect_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.effect_channel_capacity = capacity;
        self
    }

    /// Build the system and return a `Syzygy`.
    pub fn build(self) -> Syzygy<Event, Effect, Model, Resources> {
        let (core, event_tx) = Core::new(self.event_handler, self.model);
        let registry = Arc::new(self.exec_registry);

        let shell = Self::build_shell(
            registry,
            self.effect_handler,
            self.resources,
            event_tx,
            self.effect_channel_capacity,
        );
        Syzygy::new(core, shell)
    }

    fn build_shell(
        exec_registry: Arc<ExecutorRegistry<Event>>,
        effect_handler: Option<EffectHandler<Event, Effect, Resources>>,
        resources: Resources,
        event_tx: crossbeam_channel::Sender<Event>,
        effect_channel_capacity: Option<usize>,
    ) -> Shell<Event, Effect, Resources> {
        use crossbeam_channel::{bounded, unbounded};

        fn default_effect_handler<E, X, R>(_: X, _: R) -> Task<E, X>
        where
            E: Send + Sync + 'static,
            X: Send + Sync + 'static,
            R: Clone + Send + Sync + 'static,
        {
            Task::none()
        }

        let (effect_tx, effect_rx) = match effect_channel_capacity {
            Some(capacity) => bounded(capacity),
            None => unbounded(),
        };

        let effect_handler = effect_handler.unwrap_or_else(|| {
            Box::new(move |effect, resources| {
                default_effect_handler::<Event, Effect, _>(effect, resources)
            })
        });

        Shell {
            effect_rx,
            effect_tx,
            event_tx,
            effect_handler,
            resources,
            effect_channel_capacity,
            executors: exec_registry,
            closed: false,
            prefetched_effects: VecDeque::new(),
        }
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

    fn test_update(event: TestEvent, model: &mut TestModel) -> Command<TestEvent, TestEffect> {
        match event {
            TestEvent::Increment => {
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[test]
    fn test_builder_basics() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_async_executor(crate::executor::InlineAsync::<TestEvent>::new())
            .build();

        let (mut core, _shell) = runner.split();
        let _command = core.handle_event(TestEvent::Increment);
        assert_eq!(core.model().count, 1);
    }

    #[test]
    fn test_shell_type() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_async_executor(crate::executor::InlineAsync::<TestEvent>::new())
            .build();

        let (_core, shell) = runner.split();
        let _: Shell<TestEvent, TestEffect> = shell;
    }

    #[test]
    fn test_build_without_executors_is_allowed() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        // With no executors registered, processing events still works as long as
        // the effect handler does not schedule work onto an executor.
        runner
            .core()
            .try_send_event(TestEvent::Increment)
            .expect("event channel should be open");
        runner.step().unwrap();
        assert_eq!(runner.core().model().count, 1);
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
            model: &mut (UserModel, ConfigModel),
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment => {
                    let (user, config) = model;
                    user.name = "Updated".to_string();
                    config.theme = "dark".to_string();
                    Command::effect(TestEffect::Log)
                }
            }
        }

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
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_async_executor(crate::executor::InlineAsync::<TestEvent>::new())
            .build();

        let (mut core, _shell) = runner.split();

        core.handle_event(TestEvent::Increment);

        let (_user, config) = core.model();
        assert_eq!(config.theme, "dark");
    }
}
