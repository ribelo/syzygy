//! # Syzygy
//!
//! Zero-overhead event-driven state management library for Rust applications.
//!
//! Syzygy provides a zero-overhead implementation of The Elm Architecture (TEA) with
//! Core/Shell separation and a direct storage-based API, enabling deterministic state
//! management with async side effects and multi-model composition.
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
//!
//! ## New Storage-Based API
//!
//! Syzygy uses a direct storage-based approach without the need for App traits. This provides:
//!
//! - **🎯 Simplicity**: No traits to implement - just define update functions
//! - **📊 Multi-Model**: Add multiple models at compile time with full type safety
//! - **🔗 Composable**: Chain models and resources with zero-cost abstractions
//! - **⚡ Performance**: Direct storage access without runtime overhead
//!
//! ### Basic Usage
//!
//! ```rust,ignore
//! use syzygy::prelude::*;
//!
//! // Define your models
//! #[derive(Debug, Default)]
//! struct UserModel {
//!     name: String,
//!     email: String,
//! }
//!
//! #[derive(Debug, Default)]
//! struct ConfigModel {
//!     theme: String,
//! }
//!
//! // Define your update function
//! fn update(event: MyEvent, storage: &mut Storage<UserModel, Storage<ConfigModel, EmptyStorage>>)
//!     -> Command<MyEvent, MyEffect> {
//!     let user: &mut UserModel = storage.get_mut();
//!     let config: &mut ConfigModel = storage.get_mut();
//!
//!     match event {
//!         MyEvent::UpdateUser { name } => {
//!             user.name = name;
//!             Command::effect(MyEffect::SaveUser)
//!         }
//!         MyEvent::ChangeTheme { theme } => {
//!             config.theme = theme;
//!             Command::effect(MyEffect::SaveConfig)
//!         }
//!     }
//! }
//!
//! // Build your system
//! let (core, shell) = Syzygy::builder::<MyEvent, MyEffect>()
//!     .model(UserModel::default())
//!     .model(ConfigModel::default())
//!     .update(update)
//!     .build();
//!
//! // Define your effect handler
//! async fn handle_effects(effect: MyEffect, ctx: EffectContext<MyEvent>) {
//!     match effect {
//!         MyEffect::SaveUser => {
//!             println!("Saving user...");
//!             // Perform async work
//!             tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
//!             println!("User saved!");
//!         }
//!         MyEffect::SaveConfig => {
//!             println!("Saving config...");
//!             // Could send events back to Core if needed
//!             // let _ = ctx.send_event(MyEvent::ConfigSaved);
//!         }
//!     }
//! }
//!
//! // Set up the effect handler and run
//! let shell = shell.with_effect_handler(handle_effects);
//! let mut runner = Runner::new(core, shell);
//! ```
//!
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

// Core modules
pub mod async_context;
pub mod command;
pub mod core;
pub mod runner;
pub mod shell;
pub mod task;
pub mod task_collector;

// Builder pattern
pub mod builder;

// Effect handlers with AFIT
pub mod effect_handler;

// EventContext for synchronous update functions
pub mod event_context;

// Error handling
pub mod error;

// Timer abstractions for runtime neutrality
pub mod timer;

// Spawn adapters for different async runtimes
pub mod spawn;

// Storage system with UnsafeCell-based chains
pub mod storage;

// Magic handler system for automatic parameter extraction
pub mod extract;
pub mod magic_handler;

pub mod prelude {
    // Contexts for update and effect functions
    pub use crate::async_context::EffectContext;
    pub use crate::event_context::EventContext;

    // Command system
    pub use crate::command::{Command, CommandStep};

    // Core/Shell architecture
    pub use crate::core::{Core, UpdateFn};
    pub use crate::runner::{Runner, RunnerConfig, RunnerError};
    pub use crate::shell::{Shell, ShellConfig};

    // Effect handlers with AFIT
    pub use crate::effect_handler::EffectHandler;

    // Timer abstractions for runtime neutrality
    pub use crate::timer::{Time, TimeoutError, time};

    // Spawn adapters for runtime neutrality
    pub use crate::spawn::{AsyncStdSpawn, SmolSpawn, Spawn, TokioSpawn, spawner};

    // Task management
    pub use crate::task::{TaskHandle, TaskId, TaskStats, TaskTracker};

    // Storage system
    pub use crate::storage::{Chain, Contains, EmptyStorage, Storage};

    // Magic handler system
    pub use crate::extract::{FromEventContext, ModelRef, ModelMut};
    pub use crate::magic_handler::{EffectMagicHandler, EventMagicHandler, event_trigger};

    // Builder
    pub use crate::builder::{Syzygy, SyzygyBuilder};

    // Errors
    pub use crate::error::{CommandError, CoreError, ShellError};
}
