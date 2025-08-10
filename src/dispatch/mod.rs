//! Effect dispatch system
//!
//! This module contains all the machinery for creating, sending, and receiving effects.
//! Effects are the primary way to perform side effects and state mutations in Syzygy.

use crate::{context::Context, error::DispatchError};

mod bus;
mod dispatcher;

pub use bus::{EffectsBus, EffectsRx, EffectsTx};
pub use dispatcher::Dispatcher;

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
            .send(event)
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
            .send(event)
            .map_err(|_| DispatchError::new("effect channel is closed"))
    }

}

