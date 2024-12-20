use crate::{context::Context, syzygy::Syzygy};

use crossbeam_channel::{Receiver, Sender};

pub trait Effect<M>: Send + Sync + 'static {
    fn handle(self: Box<Self>, syzygy: &Syzygy<M>);
}

impl<M, F> Effect<M> for F
where
    F: FnOnce(&Syzygy<M>) + Send + Sync + 'static,
{
    fn handle(self: Box<Self>, syzygy: &Syzygy<M>) {
        (*self)(syzygy);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Runtime stopped")]
    RuntimeStopped,
}

#[derive(Debug)]
pub struct EffectBus<M> {
    pub(crate) tx: Sender<Box<dyn Effect<M>>>,
    pub(crate) rx: Receiver<Box<dyn Effect<M>>>,
}

impl<M> Clone for EffectBus<M> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            rx: self.rx.clone(),
        }
    }
}

impl<M> Default for EffectBus<M> {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }
}

impl<M> EffectBus<M> {
    pub fn dispatch<E>(&self, effect: E) -> Result<(), EffectError>
    where
        E: FnOnce(&Syzygy<M>) + Send + Sync + 'static,
    {
        let effect = Box::new(effect);
        self.tx.send(effect).map_err(|_| EffectError::ChannelClosed)
    }

    #[must_use]
    pub fn pop(&self) -> Option<Box<dyn Effect<M>>> {
        self.rx.try_recv().ok()
    }
}

pub trait DispatchEffect<M>: Sized + Context {
    fn effect_bus(&self) -> &EffectBus<M>;
    fn dispatch<E>(&self, effect: E) -> Result<(), EffectError>
    where
        E: FnOnce(&Syzygy<M>) + Send + Sync + 'static,
    {
        self.effect_bus().dispatch(effect)
    }
}
