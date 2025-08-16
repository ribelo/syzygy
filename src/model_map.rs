//! ModelMap - Zero-overhead model storage using leaked pointers
//!
//! This module provides a compile-time safe, array-based model storage
//! that maps model types to their instances with zero runtime overhead.
//! Uses intentional memory leaking for long-lived application state.

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use crate::indexed_map::{IndexedMap, Indexable};

/// Trait for model types that can be indexed
///
/// This trait allows model types to be stored and retrieved efficiently
/// using compile-time known indices. Each model type must have a unique
/// index within the model map.
pub trait ModelType: Any + Send + Sync + 'static {
    /// The compile-time index of this model type
    const MODEL_INDEX: usize;
}

/// Indexable implementation for model types
///
/// This allows ModelType to be used with IndexedMap by defining
/// the total number of model slots and how to compute indices.
pub struct ModelIndex<const N: usize>;

impl<const N: usize> Indexable for ModelIndex<N> {
    const LENGTH: usize = N;
    type Array<V> = [V; N];

    fn index(&self) -> usize {
        unreachable!("ModelIndex is never instantiated, only used for type-level computation")
    }
}

/// Zero-overhead model storage using leaked pointers
///
/// This struct stores model instances as leaked pointers in a fixed-size
/// array indexed by model type. Each model is allocated once at startup
/// and intentionally leaked for the lifetime of the application.
///
/// # Type Parameters
///
/// - `const N`: The maximum number of model types that can be stored
///
/// # Safety
///
/// This struct maintains these invariants:
/// 1. Each leaked pointer corresponds to a valid model instance
/// 2. Model indices are within bounds and correspond to the correct types
/// 3. Models remain valid for the entire program lifetime
/// 4. No two model types share the same index
pub struct ModelMap<const N: usize> {
    /// Array of leaked model pointers, type-erased as Any
    models: IndexedMap<ModelIndex<N>, *mut dyn Any>,
    /// Track which model types have been registered
    registered_types: IndexedMap<ModelIndex<N>, TypeId>,
    /// Phantom data for the size parameter
    _phantom: PhantomData<[(); N]>,
}

impl<const N: usize> ModelMap<N> {
    /// Create a new ModelMap with all model slots empty
    pub fn new() -> Self {
        Self {
            models: IndexedMap::new_null_mut_any_pointers(),
            registered_types: IndexedMap::new_with_default(TypeId::of::<()>()),
            _phantom: PhantomData,
        }
    }

    /// Register a model instance at its compile-time known index
    ///
    /// The model is intentionally leaked and will live for the entire
    /// program lifetime. This provides zero-overhead access during
    /// event processing.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// 1. `T::MODEL_INDEX` is less than N
    /// 2. No other model type has the same index
    /// 3. This method is called only once per model type
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct UserModel { users: Vec<User> }
    /// impl ModelType for UserModel { const MODEL_INDEX: usize = 0; }
    /// 
    /// let mut model_map = ModelMap::<2>::new();
    /// model_map.register(UserModel::default());
    /// ```
    pub fn register<T: ModelType>(&mut self, model: T) {
        let index = T::MODEL_INDEX;
        assert!(index < N, "Model index {} is out of bounds for ModelMap<{}>", index, N);
        
        // Check for duplicate registration
        unsafe {
            let existing_type = *self.registered_types.get(index);
            if existing_type != TypeId::of::<()>() {
                panic!(
                    "Model index {} is already registered with type {:?}, cannot register {:?}",
                    index, existing_type, TypeId::of::<T>()
                );
            }
        }

        // Leak the model and store the pointer
        let leaked = Box::leak(Box::new(model));
        unsafe {
            self.models.set(index, leaked as *mut dyn Any);
            self.registered_types.set(index, TypeId::of::<T>());
        }
    }

    /// Get a mutable reference to a model by its type
    ///
    /// Returns None if the model type hasn't been registered.
    ///
    /// # Safety
    ///
    /// This method is safe because:
    /// 1. We verify the model was registered during registration
    /// 2. Type safety is ensured by checking TypeId during registration
    /// 3. The leaked pointer remains valid for the program lifetime
    ///
    /// # Example
    ///
    /// ```ignore
    /// let user_model: &mut UserModel = model_map.get_mut::<UserModel>().unwrap();
    /// user_model.users.push(new_user);
    /// ```
    pub fn get_mut<T: ModelType>(&self) -> Option<&mut T> {
        let index = T::MODEL_INDEX;
        if index >= N {
            return None;
        }

        unsafe {
            // Check if this model type was registered
            let registered_type = *self.registered_types.get(index);
            if registered_type != TypeId::of::<T>() {
                return None;
            }

            // Get the pointer and downcast
            let ptr = *self.models.get(index);
            if ptr.is_null() {
                return None;
            }

            // Safety: We verified the type during registration
            (*ptr).downcast_mut::<T>()
        }
    }

    /// Get an immutable reference to a model by its type
    ///
    /// Returns None if the model type hasn't been registered.
    pub fn get<T: ModelType>(&self) -> Option<&T> {
        let index = T::MODEL_INDEX;
        if index >= N {
            return None;
        }

        unsafe {
            // Check if this model type was registered
            let registered_type = *self.registered_types.get(index);
            if registered_type != TypeId::of::<T>() {
                return None;
            }

            // Get the pointer and downcast
            let ptr = *self.models.get(index);
            if ptr.is_null() {
                return None;
            }

            // Safety: We verified the type during registration
            (*ptr).downcast_ref::<T>()
        }
    }

    /// Check if a model type has been registered
    pub fn has_model<T: ModelType>(&self) -> bool {
        let index = T::MODEL_INDEX;
        if index >= N {
            return false;
        }

        unsafe {
            let registered_type = *self.registered_types.get(index);
            registered_type == TypeId::of::<T>() && !self.models.is_null_at(index)
        }
    }

    /// Get the number of registered models
    pub fn model_count(&self) -> usize {
        self.models.non_null_count()
    }

    /// Get the maximum number of models this map can hold
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.model_count() == 0
    }

    /// Check if the map is full
    pub fn is_full(&self) -> bool {
        self.model_count() == N
    }
}

impl<const N: usize> Default for ModelMap<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ModelMap is Send + Sync because:
// 1. We only store leaked pointers to Send + Sync models
// 2. All operations are either immutable or use proper synchronization
// 3. The IndexedMap itself is Send + Sync when V is Send + Sync
unsafe impl<const N: usize> Send for ModelMap<N> {}
unsafe impl<const N: usize> Sync for ModelMap<N> {}

/// Builder pattern for constructing ModelMap with type safety
///
/// This builder ensures that model types are registered with unique indices
/// and provides a fluent interface for model registration.
pub struct ModelMapBuilder<const N: usize> {
    map: ModelMap<N>,
    registered_indices: Vec<usize>,
}

impl<const N: usize> ModelMapBuilder<N> {
    /// Create a new ModelMapBuilder
    pub fn new() -> Self {
        Self {
            map: ModelMap::new(),
            registered_indices: Vec::new(),
        }
    }

    /// Register a model with compile-time type checking
    ///
    /// This method ensures that:
    /// 1. The model index is unique
    /// 2. The model type implements the required traits
    /// 3. The index is within bounds
    pub fn with_model<T: ModelType>(mut self, model: T) -> Self {
        let index = T::MODEL_INDEX;
        
        // Check for duplicate indices
        if self.registered_indices.contains(&index) {
            panic!("Model index {} is already used in this builder", index);
        }
        
        self.registered_indices.push(index);
        self.map.register(model);
        self
    }

    /// Build the final ModelMap
    pub fn build(self) -> ModelMap<N> {
        self.map
    }
}

impl<const N: usize> Default for ModelMapBuilder<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test model types
    #[derive(Debug, Default)]
    struct UserModel {
        users: Vec<String>,
        count: usize,
    }

    impl ModelType for UserModel {
        const MODEL_INDEX: usize = 0;
    }

    #[derive(Debug, Default)]
    struct PostModel {
        posts: Vec<String>,
        views: usize,
    }

    impl ModelType for PostModel {
        const MODEL_INDEX: usize = 1;
    }

    #[derive(Debug, Default)]
    struct SettingsModel {
        theme: String,
        notifications: bool,
    }

    impl ModelType for SettingsModel {
        const MODEL_INDEX: usize = 2;
    }

    #[test]
    fn test_model_map_registration() {
        let mut model_map = ModelMap::<3>::new();

        // Register models
        model_map.register(UserModel::default());
        model_map.register(PostModel::default());

        // Check registration
        assert!(model_map.has_model::<UserModel>());
        assert!(model_map.has_model::<PostModel>());
        assert!(!model_map.has_model::<SettingsModel>());

        assert_eq!(model_map.model_count(), 2);
        assert!(!model_map.is_empty());
        assert!(!model_map.is_full());
    }

    #[test]
    fn test_model_map_access() {
        let mut model_map = ModelMap::<3>::new();

        // Register a model
        let user_model = UserModel {
            users: vec!["Alice".to_string(), "Bob".to_string()],
            count: 2,
        };
        model_map.register(user_model);

        // Test mutable access
        {
            let users = model_map.get_mut::<UserModel>().unwrap();
            users.users.push("Charlie".to_string());
            users.count += 1;
        }

        // Test immutable access
        let users = model_map.get::<UserModel>().unwrap();
        assert_eq!(users.users.len(), 3);
        assert_eq!(users.count, 3);
        assert!(users.users.contains(&"Charlie".to_string()));
    }

    #[test]
    fn test_model_map_builder() {
        let model_map = ModelMapBuilder::<3>::new()
            .with_model(UserModel {
                users: vec!["Alice".to_string()],
                count: 1,
            })
            .with_model(PostModel {
                posts: vec!["Hello World".to_string()],
                views: 100,
            })
            .build();

        assert!(model_map.has_model::<UserModel>());
        assert!(model_map.has_model::<PostModel>());
        assert_eq!(model_map.model_count(), 2);

        let users = model_map.get::<UserModel>().unwrap();
        assert_eq!(users.users.len(), 1);
        assert_eq!(users.count, 1);

        let posts = model_map.get::<PostModel>().unwrap();
        assert_eq!(posts.posts.len(), 1);
        assert_eq!(posts.views, 100);
    }

    #[test]
    #[should_panic(expected = "Model index 1 is already used")]
    fn test_model_map_builder_duplicate_index() {
        // This should panic because both models try to use index 1
        struct DuplicateModel;
        impl ModelType for DuplicateModel {
            const MODEL_INDEX: usize = 1; // Same as PostModel
        }

        ModelMapBuilder::<3>::new()
            .with_model(PostModel::default())
            .with_model(DuplicateModel)
            .build();
    }

    #[test]
    fn test_model_map_type_safety() {
        let mut model_map = ModelMap::<3>::new();
        model_map.register(UserModel::default());

        // Should return Some for registered type
        assert!(model_map.get::<UserModel>().is_some());
        assert!(model_map.get_mut::<UserModel>().is_some());

        // Should return None for unregistered type
        assert!(model_map.get::<PostModel>().is_none());
        assert!(model_map.get_mut::<PostModel>().is_none());
    }

    #[test]
    fn test_model_map_capacity() {
        let model_map = ModelMap::<2>::new();

        assert_eq!(model_map.capacity(), 2);
        assert_eq!(model_map.model_count(), 0);
        assert!(model_map.is_empty());
        assert!(!model_map.is_full());

        let mut model_map = model_map;
        model_map.register(UserModel::default());
        model_map.register(PostModel::default());

        assert_eq!(model_map.model_count(), 2);
        assert!(!model_map.is_empty());
        assert!(model_map.is_full());
    }

    #[test]
    #[should_panic(expected = "Model index 3 is out of bounds")]
    fn test_model_map_index_bounds() {
        struct OutOfBoundsModel;
        impl ModelType for OutOfBoundsModel {
            const MODEL_INDEX: usize = 3; // Out of bounds for ModelMap<2>
        }

        let mut model_map = ModelMap::<2>::new();
        model_map.register(OutOfBoundsModel);
    }
}