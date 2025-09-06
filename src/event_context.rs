//! # EventContext - Synchronous context for update functions
//!
//! This module provides the `EventContext`, a component that gives update functions
//! safe and controlled access to the application's model. It is designed to be
//! lightweight and synchronous.
//!
//! ## Key Components
//! - `EventContext` - The main struct that provides access to the model.
//!
//! ## Example
//! ```rust
//! # use syzygy::prelude::*;
//! # use syzygy::event_context::EventContext;
//! # #[derive(Debug, Clone)] enum TestEvent { Increment }
//! # #[derive(Debug, Clone)] enum TestEffect { Log }
//! # #[derive(Debug, Default)] struct CounterModel { count: i32 }
//! fn my_update(
//!     event: TestEvent,
//!     ctx: &mut EventContext<TestEvent, TestEffect, CounterModel>
//! ) -> Command<TestEvent, TestEffect> {
//!     let model: &mut CounterModel = ctx.model_mut();
//!     model.count += 1;
//!     Command::none()
//! }
//! ```
//!
//! This provides controlled access to the model within update functions, mirroring
//! the EffectContext pattern for consistency. EventContext is designed to be:
//! - Synchronous (unlike EffectContext)
//! - Lightweight with zero overhead
//! - Extensible for future features like debugging, tracing, etc.

/// EventContext provides controlled access to the model within update functions
///
/// This context ensures a consistent API pattern between synchronous update functions
/// and asynchronous effect handlers. Key features:
/// - Direct model access via `model()` and `model_mut()`
/// - Zero runtime overhead - just a wrapper around model reference
/// - Extensible design for future enhancements
///
/// # Example Usage
///
/// ```rust
/// # use syzygy::prelude::*;
/// # #[derive(Debug, Default)] struct MyModel { counter: i32 }
/// # #[derive(Debug, Clone)] enum MyEvent { Increment }
/// # #[derive(Debug, Clone)] enum MyEffect { Log { message: String } }
/// fn my_update(
///     event: MyEvent,
///     ctx: &mut EventContext<MyEvent, MyEffect, MyModel>
/// ) -> Command<MyEvent, MyEffect> {
///     let model: &mut MyModel = ctx.model_mut();
///     model.counter += 1;
///     Command::effect(MyEffect::Log { message: "Updated".to_string() })
/// }
/// ```
pub struct EventContext<'a, E, X, M> {
    /// Mutable reference to the model storage
    model: &'a mut M,
    /// Phantom data to ensure correct type inference
    _phantom: std::marker::PhantomData<(E, X)>,
}

impl<'a, E, X, M> EventContext<'a, E, X, M> {
    /// Create a new EventContext with access to the model
    pub fn new(model: &'a mut M) -> Self {
        Self {
            model,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get immutable reference to the model
    ///
    /// This method provides read-only access to the application's model.
    ///
    /// # Returns
    /// An immutable reference to the model
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # use syzygy::event_context::EventContext;
    /// # #[derive(Debug, Default)] struct MyModel { count: i32 }
    /// # let mut model = MyModel::default();
    /// let ctx = EventContext::<(), (), _>::new(&mut model);
    /// let model_ref: &MyModel = ctx.model();
    /// assert_eq!(model_ref.count, 0);
    /// ```
    #[must_use]
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Get mutable reference to the model
    ///
    /// This method provides read-write access to the application's model.
    ///
    /// # Returns
    /// A mutable reference to the model
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # use syzygy::event_context::EventContext;
    /// # #[derive(Debug, Default)] struct MyModel { count: i32 }
    /// # let mut model = MyModel::default();
    /// let ctx = EventContext::<(), (), _>::new(&mut model);
    /// let model_ref: &mut MyModel = ctx.model_mut();
    /// model_ref.count += 1;
    /// assert_eq!(model_ref.count, 1);
    /// ```
    #[must_use]
    pub fn model_mut(&mut self) -> &mut M {
        self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Increment,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEffect {
        Log,
    }

    #[derive(Debug, Default)]
    struct CounterModel {
        count: i32,
    }

    #[derive(Debug, Default)]
    struct UserModel {
        name: String,
    }

    #[test]
    fn test_event_context_single_model() {
        let mut model = CounterModel { count: 5 };
        let mut ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut model);

        // Test immutable access
        let model_ref: &CounterModel = ctx.model();
        assert_eq!(model_ref.count, 5);

        // Test mutable access
        let model_ref: &mut CounterModel = ctx.model_mut();
        model_ref.count += 1;
        assert_eq!(model_ref.count, 6);

        // Verify the change persisted
        let model_ref: &CounterModel = ctx.model();
        assert_eq!(model_ref.count, 6);
    }

    #[test]
    fn test_event_context_user_model() {
        let mut model = UserModel {
            name: "Alice".to_string(),
        };
        let mut ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut model);

        // Test accessing the model
        let user: &UserModel = ctx.model();
        assert_eq!(user.name, "Alice");

        // Test mutable access
        {
            let user: &mut UserModel = ctx.model_mut();
            user.name = "Bob".to_string();
        }

        // Verify changes
        let user: &UserModel = ctx.model();
        assert_eq!(user.name, "Bob");
    }
}
