//! Effect dispatch system
//!
//! This module contains all the machinery for creating, sending, and receiving effects.
//! Effects are the primary way to perform side effects and state mutations in Syzygy.

mod effect;
mod bus;
mod dispatcher;
mod traits;

pub use effect::{Effect, EffectFn};
pub use bus::{EffectsBus, EffectsTx, EffectsRx};
pub use dispatcher::Dispatcher;
pub use traits::{DispatchEffect};

#[cfg(feature = "async")]
pub use traits::AsyncDispatchExt;