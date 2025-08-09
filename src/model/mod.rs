use core::fmt;

use crate::context::Context;

mod unsync;

/// Core trait for application state models
///
/// `Model` defines the contract for state that can be managed by Syzygy.
/// Models must be thread-safe, debuggable, and provide snapshot functionality
/// for async contexts and time-travel debugging.
///
/// # Requirements
///
/// Models must implement:
/// - [`Debug`] for debugging and logging
/// - [`Send`] + [`Sync`] for thread safety
/// - `'static` lifetime for type erasure
///
/// # Snapshots
///
/// The snapshot mechanism allows for:
/// - **Async contexts**: Share read-only state with async tasks
/// - **Time-travel debugging**: Capture state at specific points
/// - **Undo/redo**: Store previous states for rollback
/// - **Testing**: Verify state changes in isolation
///
/// # Examples
///
/// Simple model with clone-based snapshots:
/// ```rust
/// use syzygy::prelude::*;
///
/// #[derive(Debug, Clone)]
/// struct CounterModel {
///     count: i32,
///     name: String,
/// }
///
/// impl Model for CounterModel {
///     type Snapshot = Self;
///     
///     fn to_snapshot(&self) -> Self::Snapshot {
///         self.clone()
///     }
/// }
/// ```
///
/// Model with optimized snapshots:
/// ```rust
/// use syzygy::prelude::*;
/// use std::sync::Arc;
///
/// #[derive(Debug)]
/// struct AppModel {
///     // Large data that's expensive to clone
///     large_data: Arc<Vec<String>>,
///     // Frequently changing data
///     counter: i32,
/// }
///
/// #[derive(Debug, Clone)]
/// struct AppSnapshot {
///     large_data: Arc<Vec<String>>, // Shared reference
///     counter: i32,                 // Cloned value
/// }
///
/// impl Model for AppModel {
///     type Snapshot = AppSnapshot;
///     
///     fn to_snapshot(&self) -> Self::Snapshot {
///         AppSnapshot {
///             large_data: Arc::clone(&self.large_data),
///             counter: self.counter,
///         }
///     }
/// }
/// ```
pub trait Model: fmt::Debug + Send + Sync + 'static {
    /// The snapshot type for this model
    ///
    /// Snapshots must be cloneable and thread-safe. They typically contain
    /// either a full clone of the model data or shared references to
    /// immutable data.
    type Snapshot: Clone + Send + Sync + 'static;

    /// Create a snapshot of the current model state
    ///
    /// This method should efficiently capture the current state in a form
    /// that can be safely shared across threads and async boundaries.
    ///
    /// # Performance Considerations
    ///
    /// - For small models, cloning the entire model is often sufficient
    /// - For large models, consider using [`Arc`] for shared data
    /// - Avoid expensive computations in this method as it may be called frequently
    ///
    /// [`Arc`]: std::sync::Arc
    fn to_snapshot(&self) -> Self::Snapshot;
}

/// Trait for read-only access to model state
///
/// `ModelAccess` provides safe, immutable access to the model within a context.
/// This is the primary way to read application state in effects and other
/// operations.
///
/// # Usage
///
/// Use `model()` to get a reference to the current state, or `query()` for
/// functional-style access with closures.
///
/// # Examples
///
/// Basic model access:
/// ```rust
/// use syzygy::prelude::*;
///
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32 }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// fn check_counter(syzygy: &Syzygy<AppModel>) -> i32 {
///     syzygy.model().counter
/// }
/// ```
///
/// Functional-style queries:
/// ```rust
/// use syzygy::prelude::*;
///
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32, name: String }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// fn format_status(syzygy: &Syzygy<AppModel>) -> String {
///     syzygy.query(|model| {
///         format!("{}: {}", model.name, model.counter)
///     })
/// }
/// ```
pub trait ModelAccess: Context {
    /// Get an immutable reference to the model
    ///
    /// This provides direct access to the current model state. The reference
    /// is valid for the lifetime of the borrow.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// # #[derive(Debug, Clone)]
    /// # struct AppModel { value: i32 }
    /// # impl Model for AppModel {
    /// #     type Snapshot = Self;
    /// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
    /// # }
    /// # let syzygy: Syzygy<AppModel> = Syzygy::builder().model(AppModel { value: 42 }).build();
    /// let current_value = syzygy.model().value;
    /// assert_eq!(current_value, 42);
    /// ```
    #[must_use]
    fn model(&self) -> &Self::Model;

    /// Query the model with a function
    ///
    /// This is a convenience method for functional-style model access.
    /// It's equivalent to calling `f(self.model())` but can be more
    /// ergonomic for complex queries.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// # #[derive(Debug, Clone)]
    /// # struct AppModel { items: Vec<String> }
    /// # impl Model for AppModel {
    /// #     type Snapshot = Self;
    /// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
    /// # }
    /// # let syzygy: Syzygy<AppModel> = Syzygy::builder().model(AppModel { items: vec!["a".to_string(), "b".to_string()] }).build();
    /// let item_count = syzygy.query(|model| model.items.len());
    /// let has_items = syzygy.query(|model| !model.items.is_empty());
    /// ```
    #[must_use]
    fn query<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Self::Model) -> R,
    {
        f(self.model())
    }
}

pub trait ModelModify: ModelAccess {
    #[must_use]
    fn model_mut(&mut self) -> &mut Self::Model;
    fn update<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self::Model) -> R,
    {
        f(self.model_mut())
    }
}

pub trait ModelSnapshotAccess: Context {
    #[must_use]
    fn snapshot(&self) -> &<<Self as Context>::Model as Model>::Snapshot;
    #[must_use]
    fn query_snapshot<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&<<Self as Context>::Model as Model>::Snapshot) -> R,
    {
        f(self.snapshot())
    }
}

pub trait ModelSnapshotCreate: Context {
    #[must_use]
    fn create_snapshot(&self) -> <<Self as Context>::Model as Model>::Snapshot;
}
