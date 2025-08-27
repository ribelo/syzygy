//! EventContext - Synchronous context for update functions
//!
//! This provides controlled access to models within update functions, mirroring
//! the EffectContext pattern for consistency. EventContext is designed to be:
//! - Synchronous (unlike EffectContext)
//! - Lightweight with zero overhead
//! - Extensible for future features like debugging, tracing, etc.

use crate::storage::Selector;

/// EventContext provides controlled access to models within update functions
///
/// This context ensures a consistent API pattern between synchronous update functions
/// and asynchronous effect handlers. Key features:
/// - Direct model access via `model()` and `model_mut()`
/// - Type-safe model selection using the Selector trait
/// - Zero runtime overhead - just a wrapper around storage reference
/// - Extensible design for future enhancements
///
/// # Example Usage
///
/// ```rust,ignore
/// fn my_update(
///     event: MyEvent,
///     ctx: EventContext<MyEvent, MyEffect, Storage<MyModel, EmptyStorage>>
/// ) -> Command<MyEvent, MyEffect> {
///     let model: &mut MyModel = ctx.model_mut();
///     model.counter += 1;
///     Command::effect(MyEffect::Log { message: "Updated".to_string() })
/// }
/// ```
pub struct EventContext<'a, Event, Effect, Storage> {
    /// Mutable reference to the model storage
    storage: &'a Storage,
    /// Phantom data to ensure correct type inference
    _phantom: std::marker::PhantomData<(Event, Effect)>,
}

impl<'a, Event, Effect, Storage> EventContext<'a, Event, Effect, Storage> {
    /// Create a new EventContext with access to model storage
    pub fn new(storage: &'a mut Storage) -> Self {
        Self {
            storage,
            _phantom: std::marker::PhantomData,
        }
    }


    /// Get immutable reference to a specific model by type
    ///
    /// This is a convenience method that delegates to the storage's get() method.
    /// The type must exist in the storage chain for this to compile.
    ///
    /// # Example
    /// ```rust,ignore
    /// let counter: &CounterModel = ctx.model();
    /// // Or with explicit type:
    /// let counter = ctx.model::<CounterModel>();
    /// ```
    #[must_use]
    pub fn model<T, Index>(&self) -> &T
    where
        Storage: Selector<T, Index>,
    {
        self.storage.get()
    }

    /// Get mutable reference to a specific model by type
    ///
    /// This is a convenience method that delegates to the storage's get_mut() method.
    /// The type must exist in the storage chain for this to compile.
    ///
    /// # Example
    /// ```rust,ignore
    /// let counter: &mut CounterModel = ctx.model_mut();
    /// counter.count += 1;
    /// // Or with explicit type:
    /// let counter = ctx.model_mut::<CounterModel>();
    /// counter.count += 1;
    /// ```
    #[must_use] pub fn model_mut<T, Index>(&self) -> &mut T
    where
        Storage: Selector<T, Index>,
    {
        self.storage.get_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::EmptyStorage;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
    }

    #[derive(Debug, Clone)]
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
        let mut storage = EmptyStorage.with_model(CounterModel { count: 5 });
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Test immutable access
        let model: &CounterModel = ctx.model();
        assert_eq!(model.count, 5);

        // Test mutable access
        let model: &mut CounterModel = ctx.model_mut();
        model.count += 1;
        assert_eq!(model.count, 6);

        // Verify the change persisted
        let model: &CounterModel = ctx.model();
        assert_eq!(model.count, 6);
    }

    #[test]
    fn test_event_context_multiple_models() {
        let mut storage = EmptyStorage
            .with_model(CounterModel { count: 10 })
            .with_model(UserModel { name: "Alice".to_string() });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Test accessing different model types
        let counter: &CounterModel = ctx.model();
        assert_eq!(counter.count, 10);

        let user: &UserModel = ctx.model();
        assert_eq!(user.name, "Alice");

        // Test mutable access to both models
        {
            let counter: &mut CounterModel = ctx.model_mut();
            counter.count = 99;

            let user: &mut UserModel = ctx.model_mut();
            user.name = "Bob".to_string();
        }

        // Verify changes
        let counter: &CounterModel = ctx.model();
        assert_eq!(counter.count, 99);

        let user: &UserModel = ctx.model();
        assert_eq!(user.name, "Bob");
    }

}