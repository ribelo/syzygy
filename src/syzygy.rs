// use bon::Builder;

use bon::Builder;

use crate::{
    context::Context,
    dispatch::{DispatchEffect, Dispatcher, EffectsBus, EffectsTx},
    event::Event,
    model::{Model, ModelAccess, ModelModify, ModelSnapshotCreate},
    resource::{ResourceAccess, ResourceModify, Resources},
};

/// The core event-driven state management system
///
/// `Syzygy` provides reactive state management with three main components:
/// - **Model**: Application state that can be read and modified
/// - **Resources**: Type-safe dependency injection for services and configuration
/// - **Effects**: Asynchronous side effects that are dispatched and processed
///
/// The name "Syzygy" refers to the astronomical alignment of celestial bodies -
/// similarly, this library aligns state, resources, and effects into a cohesive system.
///
/// # Core Concepts
///
/// ## Builder Pattern
/// Syzygy instances are created using the builder pattern:
///
/// ```rust
/// use syzygy::prelude::*;
///
/// #[derive(Debug, Clone)]
/// struct AppModel { counter: i32 }
/// impl Model for AppModel {
///     type Snapshot = Self;
///     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// }
///
/// let syzygy: Syzygy<AppModel> = Syzygy::builder()
///     .model(AppModel { counter: 0 })
///     .resource("Config { api_key: secret }".to_string())
///     .build();
/// ```
///
/// ## State Management
/// Models are accessed through trait methods that provide type-safe read/write access:
///
/// ```rust
/// # use syzygy::prelude::*;
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32 }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// # let mut syzygy: Syzygy<AppModel> = Syzygy::builder().model(AppModel { counter: 0 }).build();
/// // Read state
/// let count = syzygy.model().counter;
///
/// // Modify state
/// syzygy.update(|model| model.counter += 1);
/// ```
///
/// ## Resource Injection
/// Resources provide dependency injection for services, configuration, and shared state:
///
/// ```rust
/// # use syzygy::prelude::*;
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32 }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// # let syzygy: Syzygy<AppModel> = Syzygy::builder().model(AppModel { counter: 0 }).resource("config".to_string()).build();
/// // Access resources safely
/// if let Some(config) = syzygy.try_resource::<String>() {
///     println!("Config: {}", config);
/// }
/// ```
///
/// ## Effect System
/// Effects handle side effects and asynchronous operations through typed events:
///
/// ```rust
/// # use syzygy::prelude::*;
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32 }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// # #[derive(Debug)]
/// # struct MyEvent { amount: i32 }
/// # impl Event<AppModel> for MyEvent {
/// #     fn apply(self, ctx: &mut Syzygy<AppModel, Self>) {
/// #         ctx.update(|model| model.counter += self.amount);
/// #     }
/// # }
/// # let mut syzygy: Syzygy<AppModel, MyEvent> = Syzygy::builder().model(AppModel { counter: 0 }).build();
/// // Dispatch typed events via the DispatchEffect trait
/// use syzygy::dispatch::DispatchEffect;
/// syzygy.dispatch(MyEvent { amount: 5 });
///
/// // Process effects
/// syzygy.handle_effects();
/// ```
#[derive(Debug, Builder)]
pub struct Syzygy<M: Model, E = ()> {
    #[builder(field)]
    pub resources: Resources,
    #[builder(default)]
    pub effects_bus: EffectsBus<E>,
    #[cfg(feature = "async")]
    #[builder(default)]
    pub task_tracker: tokio_util::task::TaskTracker,
    // Note: parallel features not implemented yet
    // #[cfg(feature = "parallel")]
    // pub rayon_pool: RayonPool,
    pub model: M,
}

impl<M: Model, E, S: syzygy_builder::State> SyzygyBuilder<M, E, S> {
    /// Add a resource to the Syzygy instance being built
    ///
    /// # Examples
    ///
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// #[derive(Debug, Clone)]
    /// struct MyModel { counter: i32 }
    /// impl Model for MyModel {
    ///     type Snapshot = Self;
    ///     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
    /// }
    ///
    /// #[derive(Debug)]
    /// struct Config { max_retries: usize }
    ///
    /// let syzygy: Syzygy<MyModel> = Syzygy::builder()
    ///     .model(MyModel { counter: 0 })
    ///     .resource(Config { max_retries: 3 })
    ///     .build();
    /// ```
    pub fn resource<T>(self, resource: T) -> SyzygyBuilder<M, E, S>
    where
        T: Send + Sync + 'static,
    {
        self.resources.insert(resource);
        self
    }
}

impl<M: Model, E> Syzygy<M, E> {
    /// Process all pending effects in the queue
    ///
    /// This method drains the effect queue and executes each event in order.
    /// Events are typed messages that can read and modify the Syzygy context, including
    /// the model and resources.
    ///
    /// # Usage
    ///
    /// Call this method periodically to process dispatched effects. In a typical
    /// application, you might call this in your main loop or after dispatching
    /// a batch of effects.
    ///
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// # #[derive(Debug, Clone)]
    /// # struct AppModel { counter: i32 }
    /// # impl Model for AppModel {
    /// #     type Snapshot = Self;
    /// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
    /// # }
    /// let mut syzygy: Syzygy<AppModel> = Syzygy::builder()
    ///     .model(AppModel { counter: 0 })
    ///     .build();
    ///
    /// # #[derive(Debug)]
    /// # struct Increment { amount: i32 }
    /// # impl Event<AppModel> for Increment {
    /// #     fn apply(self, ctx: &mut Syzygy<AppModel, Self>) {
    /// #         ctx.update(|m| m.counter += self.amount);
    /// #     }
    /// # }
    /// # let mut syzygy: Syzygy<AppModel, Increment> = Syzygy::builder()
    /// #     .model(AppModel { counter: 0 })
    /// #     .build();
    /// // Dispatch some events
    /// syzygy.dispatch(Increment { amount: 1 });
    /// syzygy.dispatch(Increment { amount: 2 });
    ///
    /// // Process all events at once
    /// syzygy.handle_effects();
    ///
    /// assert_eq!(syzygy.model().counter, 3); // 0 + 1 + 2
    /// ```
    ///
    /// # Performance
    ///
    /// This method processes effects synchronously and may take time if effects
    /// perform heavy computations. Consider processing effects in smaller batches
    /// if latency is a concern.
    #[inline]
    pub fn handle_effects(&mut self)
    where
        E: Event<M>,
    {
        #[cfg(feature = "tracing")]
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "handle_effects",
            model_type = std::any::type_name::<M>(),
            event_type = std::any::type_name::<E>()
        );
        #[cfg(feature = "tracing")]
        let _enter = span.enter();

        while let Ok(event) = self.effects_bus.rx.try_recv() {
            #[cfg(feature = "tracing")]
            let effect_span = tracing::span!(tracing::Level::TRACE, "effect", kind = "event");
            
            #[cfg(feature = "tracing")]
            let _effect_enter = effect_span.enter();

            event.apply(self);
        }
    }

    /// Gracefully shutdown all spawned tasks
    ///
    /// Signals all tasks to stop and waits for them to complete
    /// up to the specified timeout.
    #[cfg(feature = "async")]
    pub async fn shutdown(&self, timeout: std::time::Duration) {
        self.task_tracker.close();
        let _ = tokio::time::timeout(timeout, self.task_tracker.wait()).await;
    }

    /// Close the task tracker to prevent new tasks from being spawned
    ///
    /// This prevents new tasks from being spawned but doesn't abort existing ones.
    /// Use `shutdown()` to wait for existing tasks to complete.
    #[cfg(feature = "async")]
    pub fn close_task_tracker(&self) {
        self.task_tracker.close();
    }

    /// Create a new dispatcher for sending effects to this Syzygy instance
    ///
    /// Dispatchers allow you to send effects from other threads or contexts.
    /// Multiple dispatchers can be created and used concurrently.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// # #[derive(Debug, Clone)]
    /// # struct AppModel { counter: i32 }
    /// # impl Model for AppModel {
    /// #     type Snapshot = Self;
    /// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
    /// # }
    /// let syzygy: Syzygy<AppModel> = Syzygy::builder()
    ///     .model(AppModel { counter: 0 })
    ///     .build();
    ///
    /// # #[derive(Debug)]
    /// # struct AddEvent { value: i32 }
    /// # impl Event<AppModel> for AddEvent {
    /// #     fn apply(self, ctx: &mut Syzygy<AppModel, Self>) {
    /// #         ctx.update(|model| model.counter += self.value);
    /// #     }
    /// # }
    /// # let syzygy: Syzygy<AppModel, AddEvent> = Syzygy::builder()
    /// #     .model(AppModel { counter: 0 })
    /// #     .build();
    /// let dispatcher = syzygy.dispatcher();
    ///
    /// // Use the dispatcher to send events
    /// dispatcher.dispatch(AddEvent { value: 5 });
    /// ```
    ///
    /// The dispatcher can be cloned and passed to other threads for concurrent
    /// effect dispatching.
    #[inline]
    pub fn dispatcher(&self) -> Dispatcher<M, E> {
        Dispatcher::new(self.effects_bus.tx.clone())
    }

    /// Spawn an async task with snapshot context
    ///
    /// Tasks are the only way to perform async operations in Syzygy.
    /// They receive a snapshot of the current state and can dispatch events back.
    /// 
    /// **Important:** Only Syzygy can spawn tasks. Tasks cannot spawn other tasks,
    /// they can only dispatch events back to the system. This enforces the principle
    /// that "events control flow, tasks execute work".
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// syzygy.task(|ctx| async move {
    ///     // Perform async operations
    ///     let data = fetch_data().await;
    ///     
    ///     // Dispatch events back (cannot spawn more tasks)
    ///     ctx.dispatch(DataFetched(data));
    /// });
    /// ```
    #[cfg(feature = "async")]
    #[inline]
    pub fn task<F, Fut>(&mut self, f: F)
    where
        F: FnOnce(crate::context::snapshot::SnapshotContext<M, E>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
        E: Send + 'static,
    {
        self.task_internal(None, f)
    }

    /// Spawn a named async task with snapshot context
    ///
    /// Like `task()` but with a name for debugging. The name should follow
    /// the pattern "module:action" (e.g., "auth:refresh_token", "data:fetch_user").
    /// 
    /// With tracing enabled, this creates a span for the task lifecycle.
    /// In debug builds without tracing, this logs task lifecycle events.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// syzygy.task_named("auth:login", |ctx| async move {
    ///     let result = login_user().await;
    ///     ctx.dispatch(LoginComplete(result));
    /// });
    /// ```
    #[cfg(feature = "async")]
    #[inline]
    pub fn task_named<F, Fut>(&mut self, name: &'static str, f: F)
    where
        F: FnOnce(crate::context::snapshot::SnapshotContext<M, E>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
        E: Send + 'static,
    {
        self.task_internal(Some(name), f)
    }

    /// Spawn a blocking task with snapshot context
    ///
    /// This wraps the blocking operation in `tokio::task::spawn_blocking`.
    /// Use this for CPU-intensive or blocking I/O operations.
    /// 
    /// Like async tasks, blocking tasks can only dispatch events back,
    /// they cannot spawn other tasks.
    #[cfg(feature = "async")]
    #[inline]
    pub fn spawn_blocking<F>(&mut self, f: F)
    where
        F: FnOnce(crate::context::snapshot::SnapshotContext<M, E>) + Send + 'static,
        E: Send + 'static,
    {
        let ctx = crate::context::snapshot::SnapshotContext::from(&*self);
        tokio::task::spawn_blocking(move || f(ctx));
    }

    /// Spawn a named blocking task with snapshot context
    ///
    /// Like `spawn_blocking()` but with a name for debugging.
    #[cfg(feature = "async")]
    #[inline]
    pub fn spawn_blocking_named<F>(&mut self, name: &'static str, f: F)
    where
        F: FnOnce(crate::context::snapshot::SnapshotContext<M, E>) + Send + 'static,
        E: Send + 'static,
    {
        let ctx = crate::context::snapshot::SnapshotContext::from(&*self);
        
        #[cfg(debug_assertions)]
        {
            log::debug!("[blocking:{}] spawning", name);
            let name = name.to_string();
            tokio::task::spawn_blocking(move || {
                log::debug!("[blocking:{}] started", name);
                let start = std::time::Instant::now();
                f(ctx);
                log::debug!("[blocking:{}] completed in {:?}", name, start.elapsed());
            });
        }
        
        #[cfg(not(debug_assertions))]
        {
            let _ = name; // Avoid unused variable warning
            tokio::task::spawn_blocking(move || f(ctx));
        }
    }

    /// Internal task spawning with immediate snapshot creation
    #[doc(hidden)]
    #[cfg(feature = "async")]
    fn task_internal<F, Fut>(&mut self, name: Option<&'static str>, f: F)
    where
        F: FnOnce(crate::context::snapshot::SnapshotContext<M, E>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
        E: Send + 'static,
    {
        let snapshot = crate::context::snapshot::SnapshotContext::from(&*self);
        
        // With tracing feature, add spans
        #[cfg(feature = "tracing")]
        {
            let future = f(snapshot);
            let future = if let Some(name) = name {
                let span = tracing::span!(
                    tracing::Level::INFO,
                    "task",
                    task.name = name,
                );
                use tracing::Instrument;
                Box::pin(future.instrument(span))
            } else {
                let span = tracing::span!(
                    tracing::Level::DEBUG,
                    "task",
                    task.anonymous = true
                );
                use tracing::Instrument;
                Box::pin(future.instrument(span))
            };
            self.task_tracker.spawn(future);
        }
        
        // Without tracing but in debug mode, add logging
        #[cfg(all(debug_assertions, not(feature = "tracing")))]
        {
            if let Some(name) = name {
                let name = name.to_string();
                let fut = f(snapshot);
                let future = async move {
                    log::debug!("[task:{}] started", name);
                    let start = std::time::Instant::now();
                    fut.await;
                    log::debug!("[task:{}] completed in {:?}", name, start.elapsed());
                };
                self.task_tracker.spawn(future);
            } else {
                self.task_tracker.spawn(f(snapshot));
            }
        }
        
        // In release without tracing, just spawn normally
        #[cfg(all(not(debug_assertions), not(feature = "tracing")))]
        {
            let _ = name; // Avoid unused variable warning
            self.task_tracker.spawn(f(snapshot));
        }
    }
}

impl<M: Model, E> Context for Syzygy<M, E> {
    type Model = M;
    type Event = E;
}

impl<M: Model, E> ModelAccess for Syzygy<M, E> {
    #[inline]
    fn model(&self) -> &M {
        &self.model
    }
}

impl<M: Model, E> ModelModify for Syzygy<M, E> {
    #[inline]
    fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }
}

impl<M: Model, E> ModelSnapshotCreate for Syzygy<M, E> {
    #[inline]
    fn create_snapshot(&self) -> <<Self as Context>::Model as Model>::Snapshot {
        self.model.to_snapshot()
    }
}

impl<M: Model, E> ResourceAccess for Syzygy<M, E> {
    #[inline]
    fn resources(&self) -> &Resources {
        &self.resources
    }
}

impl<M: Model, E> ResourceModify for Syzygy<M, E> {}

impl<M: Model, E> DispatchEffect for Syzygy<M, E> {
    #[inline]
    fn effects_tx(&self) -> &EffectsTx<E> {
        &self.effects_bus.tx
    }
}

/// RAII guard for deferred execution
///
/// `Deferred` ensures that a closure is executed when the guard is dropped,
/// implementing the "defer" pattern commonly found in Go and other languages.
/// This is useful for cleanup operations that must run regardless of how a
/// function exits (normal return, early return, or panic).
///
/// # Examples
///
/// ```rust
/// use syzygy::syzygy::defer;
///
/// fn example() {
///     let _guard = defer(|| println!("This runs when guard is dropped"));
///
///     // Do some work...
///     println!("Doing work");
///
///     // The deferred closure runs here when _guard goes out of scope
/// }
/// ```
///
/// The deferred closure can be aborted if needed:
///
/// ```rust
/// use syzygy::syzygy::defer;
///
/// fn conditional_cleanup() {
///     let guard = defer(|| println!("Cleanup"));
///
///     if some_condition() {
///         guard.abort(); // Cleanup won't run
///         return;
///     }
///
///     // Cleanup runs when guard is dropped
/// }
/// # fn some_condition() -> bool { false }
/// ```
pub struct Deferred<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Deferred<F> {
    /// Abort the deferred execution
    ///
    /// Prevents the closure from running when the `Deferred` is dropped.
    /// This is useful when the deferred action is no longer needed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use syzygy::syzygy::defer;
    ///
    /// let guard = defer(|| println!("This won't print"));
    /// guard.abort();
    /// // Nothing is printed when guard goes out of scope
    /// ```
    pub fn abort(mut self) {
        self.0.take();
    }
}

impl<F: FnOnce()> Drop for Deferred<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

#[cfg(feature = "async")]
impl<M: Model, E> Drop for Syzygy<M, E> {
    fn drop(&mut self) {
        // Close to prevent new tasks (existing tasks continue running)
        self.task_tracker.close();
    }
}

/// Create a deferred execution guard
///
/// The returned [`Deferred`] guard will execute the given closure when it's dropped,
/// unless explicitly aborted. This implements the "defer" pattern for cleanup code
/// that must run regardless of how a function exits.
///
/// # Examples
///
/// Basic usage:
/// ```rust
/// use syzygy::syzygy::defer;
///
/// fn example() {
///     let _cleanup = defer(|| println!("Cleanup executed"));
///
///     // ... do work ...
///
///     // Cleanup runs automatically when _cleanup is dropped
/// }
/// ```
///
/// With early returns:
/// ```rust
/// use syzygy::syzygy::defer;
///
/// fn process_file(filename: &str) -> Result<(), std::io::Error> {
///     let file = std::fs::File::open(filename)?;
///     let _cleanup = defer(|| println!("File processing complete"));
///
///     if filename.ends_with(".tmp") {
///         return Err(std::io::Error::new(
///             std::io::ErrorKind::InvalidInput,
///             "Temporary files not allowed"
///         ));
///         // Cleanup still runs even on early return!
///     }
///
///     // ... process file ...
///     Ok(())
/// }
/// ```
///
/// # Returns
///
/// A [`Deferred<F>`] guard that will execute the closure on drop.
/// The guard can be explicitly aborted with [`Deferred::abort()`] if needed.
#[must_use]
pub fn defer<F: FnOnce()>(f: F) -> Deferred<F> {
    Deferred(Some(f))
}
