//! Effect types and traits

use crate::{model::Model, syzygy::Syzygy};

/// Type alias for effects to make the API clearer
pub type Effect<M> = Box<dyn FnOnce(&mut Syzygy<M>) + Send + 'static>;

/// Trait for effect functions  
pub trait EffectFn<M: Model, E = ()>: FnOnce(&mut Syzygy<M, E>) + Send + 'static {}

impl<M, E, F> EffectFn<M, E> for F
where
    M: Model,
    F: FnOnce(&mut Syzygy<M, E>) + Send + 'static,
{
}
