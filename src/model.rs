use core::fmt;

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

