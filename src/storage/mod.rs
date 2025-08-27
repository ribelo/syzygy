//! Storage module providing chain-based data structures
//!
//! This module contains chain implementations for high-performance type-safe storage.

#[allow(clippy::module_inception)]
pub mod storage;

// Re-export commonly used types
pub use storage::{
    BulkExtract, BulkExtractMut, Contains, EmptyStorage, RuntimeContains, Selector, Storage,
    StorageBuilder,
};
