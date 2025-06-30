//! Effect dispatch system
//!
//! This module contains all the machinery for creating, sending, and receiving effects.
//! Effects are the primary way to perform side effects and state mutations in Syzygy.

#[cfg(feature = "async")]
use std::future::Future;

#[cfg(feature = "async")]
use tokio::sync::oneshot;

use crate::{context::Context, error::DispatchError, syzygy::Syzygy};
#[cfg(feature = "async")]
use crate::context::snapshot::SnapshotContext;

mod effect;
mod bus;
mod dispatcher;

pub use effect::{Effect, EffectFn};
pub use bus::{EffectsBus, EffectsTx, EffectsRx};
pub use dispatcher::Dispatcher;

/// Core trait for dispatching effects
/// 
/// This trait provides the fundamental `dispatch` method for sending effects.
/// It follows the principle: "There should be one—and preferably only one—obvious way to do it."
pub trait DispatchEffect: Context {
    /// Get the effects transmitter
    fn effects_tx(&self) -> &EffectsTx<Self::Model>;

    /// The primary method for dispatching effects
    /// 
    /// This is the one true way to send an effect. A closed channel is considered
    /// a fatal application error, so this method will panic if the receiver is dropped.
    #[inline]
    fn dispatch<F>(&self, effect: F)
    where
        F: EffectFn<Self::Model> + Send + Sync + 'static,
    {
        self.effects_tx()
            .send(Box::new(effect))
            .expect("Effect receiver should be active");
    }

    /// Try to dispatch an effect, returning an error if the channel is closed
    /// 
    /// This method is for advanced use cases where channel failure is recoverable.
    /// Most applications should use `dispatch` instead.
    #[inline]
    fn try_dispatch<F>(&self, effect: F) -> Result<(), DispatchError>
    where
        F: EffectFn<Self::Model> + Send + Sync + 'static,
    {
        self.effects_tx()
            .send(Box::new(effect))
            .map_err(|_| DispatchError::new("effect channel is closed"))
    }

    /// Spawn a blocking task with snapshot context
    #[cfg(feature = "async")]
    #[inline]
    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model>) + Send + Sync + 'static,
    {
        let wrapped = move |syzygy: &mut Syzygy<Self::Model>| {
            let ctx = SnapshotContext::from(syzygy);
            tokio::task::spawn_blocking(move || f(ctx));
        };
        self.dispatch(wrapped);
    }

    /// Spawn an async task with snapshot context
    #[cfg(feature = "async")]
    #[inline]
    fn task<F, Fut>(&self, f: F)
    where
        F: FnOnce(SnapshotContext<Self::Model>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let wrapped = move |syzygy: &mut Syzygy<Self::Model>| {
            let ctx = SnapshotContext::from(syzygy);
            tokio::spawn(async move {
                (f)(ctx).await;
            });
        };
        self.dispatch(wrapped);
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
    fn dispatch_sync(&self, effect: impl EffectFn<Self::Model>) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let wrapped_effect = move |ctx: &mut Syzygy<Self::Model>| {
            (effect)(ctx);
            let _ = tx.send(());
        };
        self.dispatch(wrapped_effect);
        rx
    }
}

#[cfg(feature = "async")]
impl<T: DispatchEffect> AsyncDispatchExt for T {}