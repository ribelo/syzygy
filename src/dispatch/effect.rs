//! Effect types and traits

use crate::{model::Model, syzygy::Syzygy};

/// Type alias for effects to make the API clearer
pub type Effect<M> = Box<dyn FnOnce(&mut Syzygy<M>) + Send + Sync + 'static>;

/// Trait for effect functions
pub trait EffectFn<M: Model>: FnOnce(&mut Syzygy<M>) + Send + Sync + 'static {}

impl<M, F> EffectFn<M> for F
where
    M: Model,
    F: FnOnce(&mut Syzygy<M>) + Send + Sync + 'static,
{
}