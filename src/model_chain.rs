//! Model chain system built on the generic chain foundation
//!
//! This module provides type aliases and convenience functions specifically
//! for model storage. It uses the generic chain system under the hood.

pub use crate::chain::{Chain, NoChain, ChainBuilder, Here, There, Selector};

/// Type alias for a model chain - just a Chain with more specific naming for documentation
pub type ModelChain<Head, Tail> = Chain<Head, Tail>;

/// Type alias for no models - equivalent to NoChain
pub type NoModels = NoChain;

/// Type alias for a type-erased model chain that allows runtime model discovery
///
/// This provides the benefits of the chain's cache locality while allowing
/// dynamic model access through the FindModel trait's inherent methods.
///
/// # Example
/// ```rust,ignore
/// use syzygy::model_chain::{ErasedModelChain, NoModels, ModelChainBuilder};
///
/// let chain = NoModels::default()
///     .with_model(UserModel { name: "Alice".to_string() })
///     .with_model(PostModel { title: "Hello".to_string() });
///
/// // Type erase the chain for runtime access
/// let erased: ErasedModelChain = Box::new(chain);
///
/// // Runtime model lookup using the inherent methods
/// let user = erased.find_model::<UserModel>();
/// assert_eq!(user.unwrap().name, "Alice");
/// ```
pub type ErasedModelChain = Box<dyn FindModel>;

// ============================================================================
// Recursive Model Access (Option-based)
// ============================================================================

/// Trait for recursively finding models in a chain using dynamic dispatch
///
/// This trait is object-safe and provides runtime model resolution through
/// TypeId-based lookup. The actual generic methods are implemented as inherent
/// methods on `dyn FindModel`.
pub trait FindModel: 'static {
    /// Find a model by TypeId, returning a raw pointer if found
    fn find_model_idx(&self, type_id: std::any::TypeId) -> Option<*const ()>;

    /// Find a mutable model by TypeId, returning a raw mutable pointer if found
    fn find_model_mut_dyn(&mut self, type_id: std::any::TypeId) -> Option<*mut ()>;
}

// ============================================================================
// Inherent Methods on dyn FindModel (The Magic!)
// ============================================================================

impl dyn FindModel {
    /// Find a model of type T in the chain, returning Some(&T) if found
    ///
    /// This is an inherent method on the trait object itself, which allows
    /// us to have generic methods even though the trait is object-safe.
    pub fn find_model<T: 'static>(&self) -> Option<&T> {
        let type_id = std::any::TypeId::of::<T>();
        let ptr = self.find_model_idx(type_id)?;
        // Safe because we verified the TypeId matches
        Some(unsafe { &*(ptr as *const T) })
    }

    /// Find a mutable model of type T in the chain, returning Some(&mut T) if found
    pub fn find_model_mut<T: 'static>(&mut self) -> Option<&mut T> {
        let type_id = std::any::TypeId::of::<T>();
        let ptr = self.find_model_mut_dyn(type_id)?;
        // Safe because we verified the TypeId matches
        Some(unsafe { &mut *(ptr as *mut T) })
    }
}

// Base case: NoModels has no models
impl FindModel for NoModels {
    fn find_model_idx(&self, _type_id: std::any::TypeId) -> Option<*const ()> {
        None
    }

    fn find_model_mut_dyn(&mut self, _type_id: std::any::TypeId) -> Option<*mut ()> {
        None
    }
}

// Recursive case: Check head, then tail using TypeId comparison
impl<Head, Tail> FindModel for ModelChain<Head, Tail>
where
    Head: 'static,
    Tail: FindModel,
{
    fn find_model_idx(&self, type_id: std::any::TypeId) -> Option<*const ()> {
        use std::any::TypeId;

        // Check if Head is the type we want
        if TypeId::of::<Head>() == type_id {
            // Return pointer to head as *const ()
            Some(&self.head as *const Head as *const ())
        } else {
            // Recursively search in tail
            self.tail.find_model_idx(type_id)
        }
    }

    fn find_model_mut_dyn(&mut self, type_id: std::any::TypeId) -> Option<*mut ()> {
        use std::any::TypeId;

        // Check if Head is the type we want
        if TypeId::of::<Head>() == type_id {
            // Return mutable pointer to head as *mut ()
            Some(&mut self.head as *mut Head as *mut ())
        } else {
            // Recursively search in tail
            self.tail.find_model_mut_dyn(type_id)
        }
    }
}

// ============================================================================
// Convenience methods on ModelChain for direct usage
// ============================================================================

impl<Head, Tail> ModelChain<Head, Tail>
where
    Head: 'static,
    Tail: 'static,
    Self: FindModel,
{
    /// Find a model using the runtime approach - convenience method
    pub fn find_model<T: 'static>(&self) -> Option<&T> {
        // Delegate to the inherent method on dyn FindModel
        let dyn_self: &dyn FindModel = self;
        dyn_self.find_model::<T>()
    }

    /// Find a mutable model using the runtime approach - convenience method
    pub fn find_model_mut<T: 'static>(&mut self) -> Option<&mut T> {
        // Delegate to the inherent method on dyn FindModel
        let dyn_self: &mut dyn FindModel = self;
        dyn_self.find_model_mut::<T>()
    }
}

/// Builder trait specifically for models
pub trait ModelChainBuilder<M> {
    type Output;

    /// Add a model to the chain
    fn with_model(self, model: M) -> Self::Output;
}

impl<M> ModelChainBuilder<M> for NoModels {
    type Output = ModelChain<M, NoModels>;

    fn with_model(self, model: M) -> Self::Output {
        self.with(model)
    }
}

impl<Head, Tail, M> ModelChainBuilder<M> for ModelChain<Head, Tail> {
    type Output = ModelChain<M, ModelChain<Head, Tail>>;

    fn with_model(self, model: M) -> Self::Output {
        self.with(model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct UserModel {
        name: String,
    }

    #[derive(Debug, PartialEq)]
    struct PostModel {
        title: String,
    }

    #[derive(Debug, PartialEq)]
    struct CacheModel {
        hits: u64,
    }

    #[test]
    fn test_model_chain_builder() {
        let chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() })
            .with_model(CacheModel { hits: 42 });

        // Clean syntax with type annotations on the left
        let user: &UserModel = chain.get();
        let post: &PostModel = chain.get();
        let cache: &CacheModel = chain.get();

        assert_eq!(user.name, "Alice");
        assert_eq!(post.title, "Hello");
        assert_eq!(cache.hits, 42);
    }

    #[test]
    fn test_mutable_access() {
        let mut chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() });

        // Mutable access with clean type annotations
        {
            let user: &mut UserModel = chain.get_mut();
            user.name = "Bob".to_string();
        }

        {
            let post: &mut PostModel = chain.get_mut();
            post.title = "World".to_string();
        }

        // Verify changes
        let user: &UserModel = chain.get();
        let post: &PostModel = chain.get();
        assert_eq!(user.name, "Bob");
        assert_eq!(post.title, "World");
    }

    #[test]
    fn test_turbofish_syntax() {
        let chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() });

        // Alternative turbofish syntax with index inference
        let user = chain.get::<UserModel, _>();
        let post = chain.get::<PostModel, _>();

        assert_eq!(user.name, "Alice");
        assert_eq!(post.title, "Hello");
    }

    #[test]
    fn test_head_tail_access() {
        let chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() });

        // PostModel is at the head (added last)
        assert_eq!(chain.head().title, "Hello");

        // UserModel is in the tail
        assert_eq!(chain.tail().head().name, "Alice");
    }

    #[test]
    fn test_compile_time_resolution() {
        let chain = NoModels::default()
            .with_model(UserModel { name: "Test".to_string() })
            .with_model(PostModel { title: "Test Post".to_string() });

        // These calls compile to direct field access with zero overhead
        let _user: &UserModel = chain.get();
        let _post: &PostModel = chain.get();

        // The compiler knows exactly where each type is at compile time
        // No runtime HashMap lookups, no dynamic dispatch!
    }

    #[test]
    fn test_recursive_find_model() {
        let chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() })
            .with_model(CacheModel { hits: 42 });

        // Test finding models with the new API - direct method calls on chain
        let user = chain.find_model::<UserModel>();
        assert_eq!(user.unwrap().name, "Alice");

        let post = chain.find_model::<PostModel>();
        assert_eq!(post.unwrap().title, "Hello");

        let cache = chain.find_model::<CacheModel>();
        assert_eq!(cache.unwrap().hits, 42);

        // Test that non-existent models return None
        #[derive(Debug, PartialEq)]
        struct NonExistentModel;

        let none = chain.find_model::<NonExistentModel>();
        assert!(none.is_none());
    }

    #[test]
    fn test_recursive_find_model_mut() {
        let mut chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() });

        // Test mutable finding and modification
        {
            let user = chain.find_model_mut::<UserModel>();
            user.unwrap().name = "Bob".to_string();
        }

        {
            let post = chain.find_model_mut::<PostModel>();
            post.unwrap().title = "World".to_string();
        }

        // Verify changes
        let user = chain.find_model::<UserModel>();
        assert_eq!(user.unwrap().name, "Bob");

        let post = chain.find_model::<PostModel>();
        assert_eq!(post.unwrap().title, "World");
    }

    #[test]
    fn test_dyn_find_model() {
        let chain = NoModels::default()
            .with_model(UserModel { name: "Alice".to_string() })
            .with_model(PostModel { title: "Hello".to_string() });

        // Test that we can use the chain as a trait object
        let dyn_chain: &dyn FindModel = &chain;

        // Test the magic - generic methods on trait objects!
        let user = dyn_chain.find_model::<UserModel>();
        assert_eq!(user.unwrap().name, "Alice");

        let post = dyn_chain.find_model::<PostModel>();
        assert_eq!(post.unwrap().title, "Hello");

        // Test non-existent model
        #[derive(Debug, PartialEq)]
        struct NonExistentModel;

        let none = dyn_chain.find_model::<NonExistentModel>();
        assert!(none.is_none());
    }
}
