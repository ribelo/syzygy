//! # Syzygy
//!
//! Zero-overhead event-driven state management library for Rust applications.
//!
//! Syzygy provides a zero-overhead implementation of The Elm Architecture (TEA) with
//! Core/Shell separation, enabling deterministic state management with async side effects.
//!
//! ## Runtime Support
//!
//! Syzygy takes a **"tokio-first with runtime flexibility"** approach:
//!
//! - **🥇 Tokio** (Default) - Recommended for production use
//! - **🥈 Smol** - Lightweight alternative for resource-constrained environments
//! - **🥉 Async-std** - Standard library approach
//!
//! The library achieves runtime neutrality through generic spawn functions while
//! acknowledging that tokio is the most common choice in practice.
//!
//! ```rust,ignore
//! use syzygy::prelude::*;
//!
//! // Auto-detect runtime (recommended) - zero-cost abstractions
//! runner.run_until(condition, syzygy::spawn::spawner()).await?;
//!
//! // Or be explicit
//! runner.run_until(condition, syzygy::spawn::TokioSpawn).await?;
//! ```
//!
//! ## Features
//!
//! ```toml
//! [dependencies]
//! # Default: tokio runtime
//! syzygy = { version = "0.1" }
//!
//! # Alternative runtimes
//! syzygy = { version = "0.1", default-features = false, features = ["smol"] }
//! syzygy = { version = "0.1", default-features = false, features = ["async-std"] }
//!
//! # Optional: tracing for debugging (zero overhead when disabled)
//! syzygy = { version = "0.1", features = ["tracing"] }

//! ## Effect Composition Patterns
//!
//! Syzygy supports both parallel and sequential effect execution patterns:
//!
//! ### Parallel Effects (Default)
//! ```rust,ignore
//! // All effects execute concurrently
//! Command::batch([
//!     Command::effect(FetchUserData { user_id }),
//!     Command::effect(FetchPosts { user_id }),
//!     Command::effect(FetchNotifications { user_id }),
//! ])
//! ```
//!
//! ### Sequential Effects (The Consensus Solution)
//! ```rust,ignore
//! // Effects execute one after another, stopping on first failure
//! Command::sequence([
//!     Command::effect(LoginUser { credentials }),
//!     Command::effect(FetchUserData { user_id }),
//!     Command::effect(FetchAddressData { user_id }),
//!     Command::effect(MakeASandwichForUser { user_id, preferences }),
//! ])
//!
//! // Or compose with events and effects directly
//! Command::batch([
//!     Command::effect(LoginUser { credentials }),
//!     Command::effect(FetchUserData { user_id }),
//!     Command::effect(FetchAddressData { user_id }),
//!     Command::effect(MakeASandwichForUser { user_id, preferences }),
//! ])
//! ```
//!
//! Sequential effects solve the "event-driven spaghetti" problem by eliminating
//! the need for complex event chains and manual state tracking in multi-step workflows.
//!
//! **Benefits**:
//! - ✅ Grug-friendly: Simple, readable composition
//! - ✅ Functional: Clean error handling, composable patterns
//! - ✅ Zero-overhead: Built on existing Command structure
//! - ✅ Error handling: Automatic failure propagation
//!

//! ```

// Core modules
pub mod app;
pub mod async_context;
pub mod command;
pub mod core;
pub mod shell;
pub mod task;
pub mod task_collector;
pub mod runner;

// Builder pattern
pub mod builder;

// Effect handlers with AFIT
pub mod effect_handler;

// Error handling
pub mod error;

// Timer abstractions for runtime neutrality
pub mod timer;

// Spawn adapters for different async runtimes
pub mod spawn;

// Storage system with UnsafeCell-based chains
pub mod storage;

// Model registry for multi-model storage (optional feature)
#[cfg(feature = "multi-model")]
pub mod model_registry;


pub mod prelude {
    // Core trait
    pub use crate::app::App;

    // EffectContext for controlled task spawning with safety guarantees
    pub use crate::async_context::EffectContext;

    // Command system
    pub use crate::command::{Command, CommandStep};


    // Core/Shell architecture
    pub use crate::core::Core;
    pub use crate::shell::{Shell, ShellConfig};
    pub use crate::runner::{Runner, RunnerConfig, RunnerError};

    // Effect handlers with AFIT
    pub use crate::effect_handler::EffectHandler;

    // Timer abstractions for runtime neutrality
    pub use crate::timer::{Time, TimeoutError, time};

    // Spawn adapters for runtime neutrality
    pub use crate::spawn::{Spawn, TokioSpawn, SmolSpawn, AsyncStdSpawn, spawner};

    // Task management
    pub use crate::task::{TaskTracker, TaskHandle, TaskId, TaskStats};

    // Storage system
    pub use crate::storage::{Chain, EmptyStorage, Storage, Contains};

    // Builder
    pub use crate::builder::{Syzygy, SyzygyBuilder};

    // Errors
    pub use crate::error::{CoreError, ShellError, CommandError};

    // Multi-model storage (optional feature)
    #[cfg(feature = "multi-model")]
    pub use crate::model_registry::{ModelRegistry, Model, ModelGetter};
}
