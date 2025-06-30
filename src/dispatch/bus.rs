//! Effect communication channels

use derive_more::derive::{Deref, DerefMut};

use crate::model::Model;
use super::effect::EffectFn;

/// Sender for effects
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

/// Receiver for effects
#[derive(Debug, Deref, DerefMut)]
pub struct EffectsRx<M: Model> {
    inner: crossbeam_channel::Receiver<Box<dyn EffectFn<M>>>,
}

/// Communication bus for effects
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
    /// Split the bus into sender and receiver
    #[must_use]
    pub fn split(self) -> (EffectsTx<M>, EffectsRx<M>) {
        (self.tx, self.rx)
    }
}