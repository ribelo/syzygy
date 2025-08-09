//! Stand-alone dispatcher

use super::{DispatchEffect, EffectsTx};
use crate::{context::Context, model::Model};

/// Stand-alone dispatcher that can send effects to a Syzygy instance
pub struct Dispatcher<M: Model> {
    pub tx: EffectsTx<M>,
}

impl<M: Model> Dispatcher<M> {
    /// Create a new dispatcher
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
