//! Effect dispatch system
//!
//! This module contains all the machinery for creating, sending, and receiving effects.
//! Effects are the primary way to perform side effects and state mutations in Syzygy.

#[cfg(feature = "async")]
use std::future::Future;

#[cfg(feature = "async")]
use tokio::sync::oneshot;

#[cfg(feature = "async")]
use crate::context::snapshot::SnapshotContext;
use crate::{context::Context, error::DispatchError, event::Message, syzygy::Syzygy};

mod bus;
mod dispatcher;
mod effect;

pub use bus::{EffectsBus, EffectsRx, EffectsTx};
pub use dispatcher::Dispatcher;
pub use effect::{Effect, EffectFn};

/// Core trait for dispatching effects
///
/// This trait provides the fundamental `dispatch` method for sending effects.
/// It follows the principle: "There should be one—and preferably only one—obvious way to do it."
pub trait DispatchEffect: Context {
    /// Get the effects transmitter
    fn effects_tx(&self) -> &EffectsTx<Self::Model, Self::Event>;

    /// Dispatch a typed event
    ///
    /// This method sends a typed event through the effects bus.
    /// The event must be compatible with the Syzygy's event type.
    #[inline]
    fn dispatch(&self, event: Self::Event)
    where
        Self::Event: crate::event::Event<Self::Model>,
    {
        self.effects_tx()
            .send(Message::Event(event))
            .expect("Effect receiver should be active");
    }

    /// Dispatch a closure-based effect
    ///
    /// This method dispatches closure-based effects.
    /// A closed channel is considered a fatal application error, so this method will panic if the receiver is dropped.
    #[inline]
    fn dispatch_closure<F>(&self, effect: F)
    where
        F: EffectFn<Self::Model, Self::Event> + Send + 'static,
    {
        self.effects_tx()
            .send(Message::Closure(Box::new(effect)))
            .expect("Effect receiver should be active");
    }

    /// Try to dispatch a typed event, returning an error if the channel is closed
    ///
    /// This method is for advanced use cases where channel failure is recoverable.
    /// Most applications should use `dispatch` instead.
    #[inline]
    fn try_dispatch(&self, event: Self::Event) -> Result<(), DispatchError>
    where
        Self::Event: crate::event::Event<Self::Model>,
    {
        self.effects_tx()
            .send(Message::Event(event))
            .map_err(|_| DispatchError::new("effect channel is closed"))
    }

    /// Try to dispatch a closure effect, returning an error if the channel is closed
    ///
    /// This method is for advanced use cases where channel failure is recoverable.
    /// Most applications should use `dispatch_closure` instead.
    #[inline]
    fn try_dispatch_closure<F>(&self, effect: F) -> Result<(), DispatchError>
    where
        F: EffectFn<Self::Model, Self::Event> + Send + 'static,
    {
        self.effects_tx()
            .send(Message::Closure(Box::new(effect)))
            .map_err(|_| DispatchError::new("effect channel is closed"))
    }

    /// Spawn an async task with snapshot context
    ///
    /// This method spawns an async task that receives a snapshot of the current context.
    /// The task is tracked and will be cancelled on shutdown.
    ///
    /// When called from a dispatcher or snapshot, this creates the snapshot 
    /// when the effect is processed (inside `handle_effects`). This is necessary
    /// because trait methods only have access to the effects channel.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// syzygy.task(|ctx| async move {
    ///     // Perform async operations
    ///     let data = fetch_data().await;
    ///     
    ///     // Dispatch effects back to the main context
    ///     ctx.dispatch_closure(|syzygy| {
    ///         syzygy.update(|model| model.data = data);
    ///     });
    /// });
    /// ```
    #[cfg(feature = "async")]
    #[inline]
    fn task<F, Fut>(&self, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model, Self::Event>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        Self::Event: Send + 'static,
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
    fn task_named<F, Fut>(&self, name: &'static str, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model, Self::Event>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        Self::Event: Send + 'static,
    {
        self.task_internal(Some(name), f)
    }

    /// Internal task spawning with optional name and tracing support
    #[doc(hidden)]
    #[cfg(feature = "async")]
    fn task_internal<F, Fut>(&self, name: Option<&'static str>, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model, Self::Event>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
        Self::Event: Send + 'static,
    {
        // Send as Message::Closure that creates snapshot and spawns task at execution time
        self.dispatch_closure(move |syzygy: &mut Syzygy<Self::Model, Self::Event>| {
            let tracker = syzygy.task_tracker.clone();
            let ctx = SnapshotContext::from(syzygy);
            
            // With tracing feature, add spans
            #[cfg(feature = "tracing")]
            {
                let future = f(ctx);
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
                tracker.spawn(future);
            }
            
            // Without tracing but in debug mode, add logging
            #[cfg(all(debug_assertions, not(feature = "tracing")))]
            {
                if let Some(name) = name {
                    let name = name.to_string();
                    let fut = f(ctx);
                    let future = async move {
                        log::debug!("[task:{}] started", name);
                        let start = std::time::Instant::now();
                        fut.await;
                        log::debug!("[task:{}] completed in {:?}", name, start.elapsed());
                    };
                    tracker.spawn(future);
                } else {
                    tracker.spawn(f(ctx));
                }
            }
            
            // In release without tracing, just spawn normally
            #[cfg(all(not(debug_assertions), not(feature = "tracing")))]
            {
                let _ = name; // Avoid unused variable warning
                tracker.spawn(f(ctx));
            }
        });
    }

    /// Spawn a blocking task with snapshot context
    ///
    /// This wraps the blocking operation in `tokio::task::spawn_blocking`.
    /// Use this for CPU-intensive or blocking I/O operations.
    #[cfg(feature = "async")]
    #[inline]
    fn spawn_blocking<F>(&self, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model, Self::Event>) + Send + 'static,
        Self::Event: Send + 'static,
    {
        let wrapped = move |syzygy: &mut Syzygy<Self::Model, Self::Event>| {
            let ctx = SnapshotContext::from(syzygy);
            tokio::task::spawn_blocking(move || f(ctx));
        };
        self.dispatch_closure(wrapped);
    }

    /// Spawn a named blocking task with snapshot context
    ///
    /// Like `spawn_blocking()` but with a name for debugging. The name should follow
    /// the pattern "module:action" (e.g., "compute:hash_password", "io:read_file").
    /// 
    /// In debug builds, this logs task lifecycle events. In release builds,
    /// the name is ignored for zero overhead.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// syzygy.spawn_blocking_named("compute:hash", |ctx| {
    ///     let hash = expensive_computation();
    ///     ctx.dispatch_closure(move |s| s.update(|m| m.hash = hash));
    /// });
    /// ```
    #[cfg(feature = "async")]
    #[inline]
    fn spawn_blocking_named<F>(&self, name: &'static str, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model, Self::Event>) + Send + 'static,
        Self::Event: Send + 'static,
    {
        let wrapped = move |syzygy: &mut Syzygy<Self::Model, Self::Event>| {
            let ctx = SnapshotContext::from(syzygy);
            
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
        };
        self.dispatch_closure(wrapped);
    }
}

/// Extension trait for async-specific dispatch functionality
///
/// This trait is separate to keep the core `DispatchEffect` trait clean and focused.
/// It contains specialized methods for bridging sync effects with async code.
#[cfg(feature = "async")]
pub trait AsyncDispatchExt: DispatchEffect {
    /// Dispatch an effect synchronously from async context
    ///
    /// Returns a receiver that will be notified when the effect completes.
    /// This is primarily useful for testing or bridging sync effects with async code.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let completion = syzygy.dispatch_sync(|ctx| {
    ///     ctx.update(|model| model.counter += 1);
    /// });
    ///
    /// // In async context
    /// completion.await.unwrap();
    /// ```
    #[must_use]
    #[inline]
    fn dispatch_sync(&self, effect: impl EffectFn<Self::Model, Self::Event>) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let wrapped_effect = move |ctx: &mut Syzygy<Self::Model, Self::Event>| {
            (effect)(ctx);
            let _ = tx.send(());
        };
        self.dispatch_closure(wrapped_effect);
        rx
    }
}

#[cfg(feature = "async")]
impl<T: DispatchEffect> AsyncDispatchExt for T {}
