//! Storage module providing chain-based data structures
//!
//! This module contains chain implementations for high-performance type-safe storage.

pub mod generic;

#[allow(clippy::module_inception)]
pub mod storage;

// Re-export commonly used types
pub use storage::{
    EmptyStorage, Storage, StorageBuilder,
};

// Re-export the traits from generic module
pub use generic::{Contains, RuntimeContains, Selector};

// Re-export generic types for direct usage
pub use generic::{
    Chain, EmptyChain, ChainBuilder, Here, There,
};

/// Helper macro to create nested storage chains from a list of types.
///
/// This macro simplifies creating complex storage chains by allowing you to
/// specify types in a flat list rather than deeply nested `Storage<>` types.
///
/// # Examples
///
/// ```rust
/// use syzygy::storage::*;
///
/// #[derive(Debug)]
/// struct Model1(i32);
/// #[derive(Debug)]
/// struct Model2(String);
/// #[derive(Debug)]  
/// struct Model3(bool);
///
/// // Instead of writing:
/// // Storage<Model3, Storage<Model2, Storage<Model1, EmptyStorage>>>
/// 
/// // You can write:
/// type MyStorage = storage_type!(Model1, Model2, Model3);
///
/// let storage = EmptyStorage::new()
///     .with_model(Model1(42))
///     .with_model(Model2("hello".to_string()))
///     .with_model(Model3(true));
///
/// let _: MyStorage = storage;
/// ```
///
/// The macro processes types from left to right, with the leftmost type
/// being first added to the chain and the rightmost type being last added.
/// This matches the natural order when using `.with_model()` calls.
#[macro_export]
macro_rules! storage_type {
    // Entry point - reverse the order and process
    ($($types:ty),+) => {
        storage_type!(@reverse [] $($types),+)
    };
    
    // Reverse the types into an accumulator
    (@reverse [$($acc:ty),*]) => {
        storage_type!(@build $($acc),*)
    };
    (@reverse [$($acc:ty),*] $head:ty $(, $tail:ty)*) => {
        storage_type!(@reverse [$head $(, $acc)*] $($tail),*)
    };
    
    // Build the nested Storage type from reversed order
    (@build $type:ty) => {
        $crate::storage::Storage<$type, $crate::storage::EmptyStorage>
    };
    (@build $first:ty, $($rest:ty),+) => {
        $crate::storage::Storage<$first, storage_type!(@build $($rest),+)>
    };
}

// Re-export the macro at the module level
pub use storage_type;
