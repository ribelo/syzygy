//! Stand-alone dispatcher

use std::marker::PhantomData;

use super::{DispatchEffect, EffectsTx};
use crate::{context::Context, model::Model};

/// Stand-alone dispatcher that can send effects to a Syzygy instance
pub struct Dispatcher<M: Model, E> {
    pub tx: EffectsTx<M, E>,
    _phantom: PhantomData<M>,
}

impl<M: Model, E> Dispatcher<M, E> {
    /// Create a new dispatcher
    #[must_use]
    pub fn new(tx: EffectsTx<M, E>) -> Self {
        Self { 
            tx,
            _phantom: PhantomData,
        }
    }
}

impl<M: Model, E> Context for Dispatcher<M, E> {
    type Model = M;
    type Event = E;
}

impl<M: Model, E> DispatchEffect for Dispatcher<M, E> {
    fn effects_tx(&self) -> &EffectsTx<Self::Model, Self::Event> {
        &self.tx
    }
}
