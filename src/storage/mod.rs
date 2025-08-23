//! Storage module providing chain-based data structures
//!
//! This module contains chain implementations for high-performance type-safe storage.

pub mod chain;
pub mod storage;

// Re-export commonly used types, avoiding ambiguity
pub use chain::{Chain, Selector as ChainSelector};
pub use storage::{Storage, Selector, EmptyStorage, Contains, RuntimeContains, DuplicateTypeError};
