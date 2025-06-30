use bon::Builder;

use crate::{
    context::Context,
    dispatch::{DispatchEffect, Dispatcher, EffectsBus, EffectsTx},
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
/// let syzygy = Syzygy::builder()
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
/// # let mut syzygy = Syzygy::builder().model(AppModel { counter: 0 }).build();
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
/// # let syzygy = Syzygy::builder().model(AppModel { counter: 0 }).resource("config".to_string()).build();
/// // Access resources safely
/// if let Some(config) = syzygy.try_resource::<String>() {
///     println!("Config: {}", config);
/// }
/// ```
///
/// ## Effect System
/// Effects handle side effects and asynchronous operations:
///
/// ```rust
/// # use syzygy::prelude::*;
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32 }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// # let mut syzygy = Syzygy::builder().model(AppModel { counter: 0 }).build();
/// // Dispatch an effect
/// syzygy.dispatch(|ctx| {
///     ctx.update(|model| model.counter += 10);
/// });
///
/// // Process effects
/// syzygy.handle_effects();
/// ```
#[derive(Debug, Builder)]
pub struct Syzygy<M: Model> {
    #[builder(field)]
    pub resources: Resources,
    #[builder(field)]
    pub effects_bus: EffectsBus<M>,
    // Note: parallel features not implemented yet
    // #[cfg(feature = "parallel")]
    // #[builder(into)]
    // pub rayon_pool: RayonPool,
    pub model: M,
}

impl<M: Model, S: syzygy_builder::State> SyzygyBuilder<M, S> {
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
    /// let syzygy = Syzygy::builder()
    ///     .model(MyModel { counter: 0 })
    ///     .resource(Config { max_retries: 3 })
    ///     .build();
    /// ```
    pub fn resource<T>(mut self, resource: T) -> SyzygyBuilder<M, S>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        self.resources.insert(resource);
        self
    }
}

impl<M: Model> Syzygy<M> {
    /// Process all pending effects in the queue
    ///
    /// This method drains the effect queue and executes each effect in order.
    /// Effects are closures that can read and modify the Syzygy context, including
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
    /// let mut syzygy = Syzygy::builder()
    ///     .model(AppModel { counter: 0 })
    ///     .build();
    ///
    /// // Dispatch some effects
    /// syzygy.dispatch(|ctx| ctx.update(|m| m.counter += 1));
    /// syzygy.dispatch(|ctx| ctx.update(|m| m.counter *= 2));
    ///
    /// // Process all effects at once
    /// syzygy.handle_effects();
    /// 
    /// assert_eq!(syzygy.model().counter, 2); // (0 + 1) * 2
    /// ```
    ///
    /// # Performance
    ///
    /// This method processes effects synchronously and may take time if effects
    /// perform heavy computations. Consider processing effects in smaller batches
    /// if latency is a concern.
    pub fn handle_effects(&mut self) {
        while let Ok(effect) = self.effects_bus.rx.try_recv() {
            (effect)(self);
        }
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
    /// let syzygy = Syzygy::builder()
    ///     .model(AppModel { counter: 0 })
    ///     .build();
    ///
    /// let dispatcher = syzygy.dispatcher();
    /// 
    /// // Use the dispatcher to send effects
    /// dispatcher.dispatch(|ctx| {
    ///     ctx.update(|model| model.counter += 5);
    /// });
    /// ```
    ///
    /// The dispatcher can be cloned and passed to other threads for concurrent
    /// effect dispatching.
    pub fn dispatcher(&self) -> Dispatcher<M> {
        Dispatcher::new(self.effects_bus.tx.clone())
    }
}

impl<M: Model> Context for Syzygy<M> {
    type Model = M;
}

impl<M: Model> ModelAccess for Syzygy<M> {
    #[inline]
    fn model(&self) -> &M {
        &self.model
    }
}

impl<M: Model> ModelModify for Syzygy<M> {
    #[inline]
    fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }
}

impl<M: Model> ModelSnapshotCreate for Syzygy<M> {
    #[inline]
    fn create_snapshot(&self) -> <<Self as Context>::Model as Model>::Snapshot {
        self.model.to_snapshot()
    }
}

impl<M: Model> ResourceAccess for Syzygy<M> {
    #[inline]
    fn resources(&self) -> &Resources {
        &self.resources
    }
}

impl<M: Model> ResourceModify for Syzygy<M> {}

impl<M: Model> DispatchEffect for Syzygy<M> {
    #[inline]
    fn effects_tx(&self) -> &EffectsTx<M> {
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