//! Effect communication channels

use derive_more::derive::{Deref, DerefMut};

use crate::{event::Message, model::Model};

/// Sender for effects
#[derive(Debug, Deref)]
pub struct EffectsTx<M: Model, E> {
    inner: crossbeam_channel::Sender<Message<M, E>>,
}

impl<M: Model, E> Clone for EffectsTx<M, E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Receiver for effects
#[derive(Debug, Deref, DerefMut)]
pub struct EffectsRx<M: Model, E> {
    inner: crossbeam_channel::Receiver<Message<M, E>>,
}

/// Communication bus for effects
#[derive(Debug)]
pub struct EffectsBus<M: Model, E> {
    pub(crate) tx: EffectsTx<M, E>,
    pub(crate) rx: EffectsRx<M, E>,
}

impl<M: Model, E> Default for EffectsBus<M, E> {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx: EffectsTx { inner: tx },
            rx: EffectsRx { inner: rx },
        }
    }
}

impl<M: Model, E> EffectsBus<M, E> {
    /// Split the bus into sender and receiver
    #[must_use]
    pub fn split(self) -> (EffectsTx<M, E>, EffectsRx<M, E>) {
        (self.tx, self.rx)
    }
}
