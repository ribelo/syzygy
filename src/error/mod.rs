//! Error types for Syzygy operations
//!
//! This module provides specific error types for different parts of the system.
//! Each module defines errors that can occur in its domain.

pub mod command;
pub mod core;
pub mod shell;

// Re-export commonly used error types
pub use command::CommandError;
pub use core::CoreError;
pub use shell::ShellError;
