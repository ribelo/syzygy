//! Event system for Syzygy
//!
//! This module provides the typed event system that allows for debuggable,
//! testable message passing.

use crate::{model::Model, syzygy::Syzygy};

/// Core trait for typed events that can be applied to Syzygy
///
/// Events are the primary way to trigger state changes in Syzygy.
/// They provide a typed, debuggable way to modify state and dispatch effects.
///
/// # Examples
///
/// ```rust
/// use syzygy::prelude::*;
///
/// #[derive(Debug, Clone)]
/// struct Increment { amount: i32 }
///
/// impl<M: Model> Event<M> for Increment {
///     fn apply(self, syzygy: &mut Syzygy<M, Self>) {
///         // Apply the event logic
///     }
/// }
/// ```
pub trait Event<M: Model>: Send + 'static {
    /// Apply this event to the Syzygy instance
    fn apply(self, syzygy: &mut Syzygy<M, Self>)
    where
        Self: Sized;
}

/// Implementation for unit type - allows using Syzygy without typed events
impl<M: Model> Event<M> for () {
    fn apply(self, _syzygy: &mut Syzygy<M, ()>) {
        // Unit type does nothing when applied
    }
}