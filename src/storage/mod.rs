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
