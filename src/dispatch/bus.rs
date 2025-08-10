//! Effect communication channels

/// Sender for effects
#[derive(Debug)]
pub struct EffectsTx<E> {
    inner: crossbeam_channel::Sender<E>,
}

impl<E> EffectsTx<E> {
    /// Send an event through the channel
    pub fn send(&self, event: E) -> Result<(), crossbeam_channel::SendError<E>> {
        self.inner.send(event)
    }
}

impl<E> Clone for EffectsTx<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Receiver for effects
#[derive(Debug)]
pub struct EffectsRx<E> {
    inner: crossbeam_channel::Receiver<E>,
}

impl<E> EffectsRx<E> {
    /// Try to receive an event from the channel
    pub fn try_recv(&self) -> Result<E, crossbeam_channel::TryRecvError> {
        self.inner.try_recv()
    }
}

/// Communication bus for effects
#[derive(Debug)]
pub struct EffectsBus<E> {
    pub(crate) tx: EffectsTx<E>,
    pub(crate) rx: EffectsRx<E>,
}

impl<E> Default for EffectsBus<E> {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx: EffectsTx { inner: tx },
            rx: EffectsRx { inner: rx },
        }
    }
}

impl<E> EffectsBus<E> {
    /// Split the bus into sender and receiver
    #[must_use]
    pub fn split(self) -> (EffectsTx<E>, EffectsRx<E>) {
        (self.tx, self.rx)
    }
}
