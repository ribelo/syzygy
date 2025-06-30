#[cfg(feature = "async")]
use std::future::Future;

use derive_more::derive::{Deref, DerefMut};
#[cfg(feature = "async")]
use tokio::sync::oneshot;

use crate::{context::Context, error::DispatchError};
use crate::model::ModelModify;
#[cfg(feature = "async")]
use crate::prelude::SnapshotContext;
use crate::{model::Model, syzygy::Syzygy};

/// Type alias for effects to make the API clearer
pub type Effect<M> = Box<dyn FnOnce(&mut Syzygy<M>) + Send + Sync + 'static>;

pub trait EffectFn<M: Model>: FnOnce(&mut Syzygy<M>) + Send + Sync + 'static {}

impl<M, F> EffectFn<M> for F
where
    M: Model,
    F: FnOnce(&mut Syzygy<M>) + Send + Sync + 'static,
{
}

#[derive(Debug, Deref)]
pub struct EffectsTx<M: Model> {
    inner: crossbeam_channel::Sender<Box<dyn EffectFn<M>>>,
}

impl<M: Model> Clone for EffectsTx<M> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Deref, DerefMut)]
pub struct EffectsRx<M: Model> {
    inner: crossbeam_channel::Receiver<Box<dyn EffectFn<M>>>,
}

#[derive(Debug)]
pub struct EffectsBus<M: Model> {
    pub(crate) tx: EffectsTx<M>,
    pub(crate) rx: EffectsRx<M>,
}

impl<M: Model> Default for EffectsBus<M> {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx: EffectsTx { inner: tx },
            rx: EffectsRx { inner: rx },
        }
    }
}

impl<M: Model> EffectsBus<M> {
    #[must_use]
    pub fn split(self) -> (EffectsTx<M>, EffectsRx<M>) {
        (self.tx, self.rx)
    }
}

pub struct Dispatcher<M: Model> {
    pub tx: EffectsTx<M>,
}

impl<M: Model> Dispatcher<M> {
    #[must_use]
    pub fn new(tx: EffectsTx<M>) -> Self {
        Self { tx }
    }
}

impl<M: Model> Context for Dispatcher<M> {
    type Model = M;
}

impl<M: Model> DispatchEffect for Dispatcher<M> {
    fn effects_tx(&self) -> &EffectsTx<Self::Model> {
        &self.tx
    }
}

pub trait DispatchEffect: Context {
    fn effects_tx(&self) -> &EffectsTx<Self::Model>;

    #[inline]
    fn send_effect<F>(&self, effect: F)
    where
        F: EffectFn<Self::Model> + Send + Sync + 'static,
    {
        self.effects_tx()
            .send(Box::new(effect))
            .expect("Effect receiver should be active");
    }

    /// Try to send an effect, returning an error if the channel is closed
    #[inline]
    fn try_send_effect<F>(&self, effect: F) -> Result<(), DispatchError>
    where
        F: EffectFn<Self::Model> + Send + Sync + 'static,
    {
        self.effects_tx()
            .send(Box::new(effect))
            .map_err(|_| DispatchError::new("effect channel is closed"))
    }

    #[inline]
    fn dispatch<F>(&self, effect: F)
    where
        F: EffectFn<Self::Model> + Send + Sync + 'static,
    {
        self.send_effect(effect);
    }

    /// Try to dispatch an effect, returning an error if it fails
    #[inline]
    fn try_dispatch<F>(&self, effect: F) -> Result<(), DispatchError>
    where
        F: EffectFn<Self::Model> + Send + Sync + 'static,
    {
        self.try_send_effect(effect)
    }

    #[cfg(feature = "async")]
    #[must_use]
    #[inline]
    fn dispatch_sync(&self, effect: impl EffectFn<Self::Model>) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let wrapped_effect = move |ctx: &mut Syzygy<Self::Model>| {
            (effect)(ctx);
            let _ = tx.send(());
        };
        self.send_effect(wrapped_effect);
        rx
    }

    #[inline]
    fn dispatch_update<F>(&self, update: F)
    where
        F: FnOnce(&mut Self::Model) + Send + Sync + 'static,
    {
        let wrapped = move |ctx: &mut Syzygy<Self::Model>| {
            ctx.update(update);
        };
        self.send_effect(wrapped);
    }

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
        self.send_effect(wrapped);
    }

    /// Dispatch with builder pattern
    #[inline]
    fn dispatch_builder<F>(&self, f: F)
    where
        F: FnOnce() -> crate::effect_builder::EffectBuilder<Self::Model>,
    {
        self.dispatch(f().build());
    }

    /// Convenience method for timed effects
    #[inline]
    fn dispatch_timed<F>(&self, name: &'static str, effect: F)
    where
        F: EffectFn<Self::Model> + 'static,
    {
        use crate::effect_builder::EffectExt;
        self.dispatch(effect.timed(name).build());
    }

    /// Convenience method for traced effects
    #[inline]
    fn dispatch_traced<F>(&self, effect: F)
    where
        F: EffectFn<Self::Model> + 'static,
    {
        use crate::effect_builder::EffectExt;
        self.dispatch(effect.traced().build());
    }

    /// Convenience method for named effects
    #[inline]
    fn dispatch_named<F>(&self, name: &'static str, effect: F)
    where
        F: EffectFn<Self::Model> + 'static,
    {
        use crate::effect_builder::EffectExt;
        self.dispatch(effect.named(name).build());
    }
}
