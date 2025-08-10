//! Effect communication channels

use std::marker::PhantomData;

use crate::model::Model;

/// Sender for effects
#[derive(Debug)]
pub struct EffectsTx<M: Model, E> {
    #[allow(dead_code)]
    inner: crossbeam_channel::Sender<E>,
    _phantom: PhantomData<M>,
}

impl<M: Model, E> EffectsTx<M, E> {
    /// Send an event through the channel
    pub fn send(&self, event: E) -> Result<(), crossbeam_channel::SendError<E>> {
        self.inner.send(event)
    }
}

impl<M: Model, E> Clone for EffectsTx<M, E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _phantom: PhantomData,
        }
    }
}

/// Receiver for effects
#[derive(Debug)]
pub struct EffectsRx<M: Model, E> {
    #[allow(dead_code)]
    inner: crossbeam_channel::Receiver<E>,
    _phantom: PhantomData<M>,
}

impl<M: Model, E> EffectsRx<M, E> {
    /// Try to receive an event from the channel
    pub fn try_recv(&self) -> Result<E, crossbeam_channel::TryRecvError> {
        self.inner.try_recv()
    }
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
            tx: EffectsTx { inner: tx, _phantom: PhantomData },
            rx: EffectsRx { inner: rx, _phantom: PhantomData },
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
