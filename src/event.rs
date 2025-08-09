//! Event system for Syzygy
//!
//! This module provides the typed event system that allows for debuggable,
//! testable message passing while maintaining the flexibility of closures.

use crate::{model::Model, syzygy::Syzygy};

/// Core trait for typed events that can be applied to Syzygy
///
/// Events are the primary way to trigger state changes in Syzygy.
/// They provide a typed, debuggable alternative to closures while
/// maintaining flexibility through the Message enum.
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

/// Internal message type that wraps all possible effect types
///
/// This enum is used internally by Syzygy to handle different types of
/// effects through a unified channel. Users typically don't interact with
/// this type directly.
pub enum Message<M: Model, E> {
    /// A typed event that implements the Event trait
    Event(E),
    
    /// A closure-based effect for rapid prototyping and migration
    Closure(Box<dyn FnOnce(&mut Syzygy<M, E>) + Send>),
}


impl<M: Model, E> Message<M, E> {
    /// Create a new closure message
    pub fn closure<F>(f: F) -> Self
    where
        F: FnOnce(&mut Syzygy<M, E>) + Send + 'static,
    {
        Message::Closure(Box::new(f))
    }
}