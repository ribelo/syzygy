#![feature(downcast_unchecked)]
//! # Syzygy - Zero-Overhead TEA for Rust
//!
//! A high-performance implementation of The Elm Architecture (TEA) with Core/Shell separation,
//! providing deterministic state management with async side effects and multi-model composition.
//!
//! ## Quick Start
//!
//! ```rust
//! use syzygy::executor::{InlineAsync, Task};
//! use syzygy::prelude::*;
//!
//! #[derive(Default)]
//! struct CounterModel {
//!     count: i32,
//! }
//!
//! #[derive(Clone)]
//! enum CounterEvent {
//!     Increment,
//!     Decrement,
//! }
//!
//! #[derive(Clone)]
//! enum CounterEffect {
//!     Log(String),
//! }
//!
//! #[derive(Clone)]
//! struct AppResources {
//!     prefix: &'static str,
//! }
//!
//! fn event_handler(
//!     event: CounterEvent,
//!     model: &mut CounterModel,
//! ) -> Command<CounterEvent, CounterEffect> {
//!     match event {
//!         CounterEvent::Increment => {
//!             model.count += 1;
//!             Command::effect(CounterEffect::Log(format!("{}", model.count)))
//!         }
//!         CounterEvent::Decrement => {
//!             model.count -= 1;
//!             Command::effect(CounterEffect::Log(format!("{}", model.count)))
//!         }
//!     }
//! }
//!
//! fn effect_handler(
//!     effect: CounterEffect,
//!     resources: AppResources,
//! ) -> Task<CounterEvent, CounterEffect> {
//!     match effect {
//!         CounterEffect::Log(message) => Task::async_on::<InlineAsync<CounterEvent>, _>(
//!             async move {
//!                 println!("{} {message}", resources.prefix);
//!                 Command::none()
//!             },
//!         ),
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
//!     .model(CounterModel::default())
//!     .with_resources(AppResources { prefix: "LOG" })
//!     .event_handler(event_handler)
//!     .effect_handler(effect_handler)
//!     .with_async_executor(InlineAsync::<CounterEvent>::new())
//!     .build();
//!
//! runner.core().send_event(CounterEvent::Increment)?;
//! runner.run_until(|core, _shell| core.model().count == 1)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Core Features
//!
//! ### 🚀 **Zero-Overhead Performance**
//! - Direct storage access without runtime overhead
//! - Compile-time type safety with zero-cost abstractions
//! - Task spawning 24x faster than alternatives (~4ns per task)
//!
//! ### 🏗️ **Multi-Model Architecture**
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default)] struct UserModel { name: String }
//! # #[derive(Debug, Default)] struct ConfigModel { theme: String }
//! # #[derive(Debug, Clone)] enum Event { Test }
//! # #[derive(Debug, Clone)] enum Effect { Test }
//! # fn update(e: Event, model: &mut (ConfigModel, UserModel)) -> Command<Event, Effect> { Command::none() }
//! # fn effects(_e: Effect, _resources: ()) -> Task<Event, Effect> { Task::none() }
//! let (core, shell) = Syzygy::builder()
//!     .model(UserModel::default())     // Add multiple models
//!     .model(ConfigModel::default())   // Type-safe composition
//!     .event_handler(update)
//!     .effect_handler(effects)
//!     .build();
//! ```
//!
//! ### 🎯 **Magic Handlers** (Axum-style Parameter Injection)
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default)] struct UserModel { name: String }
//! # #[derive(Debug, Default)] struct ConfigModel { theme: String }
//! # #[derive(Debug, Clone)] enum Event { UpdateUser { name: String } }
//! # #[derive(Debug, Clone)] enum Effect { SaveUser }
//! // Automatically extract what you need - no boilerplate!
//! fn handle_user_update(
//!     event: Event,
//!     user: &mut UserModel,    // Automatic extraction
//!     config: &ConfigModel,    // Mix read-only and mutable
//! ) -> Command<Event, Effect> {
//!     match event {
//!         Event::UpdateUser { name } => {
//!             user.name = name;
//!             println!("Updated user in {} theme", config.theme);
//!             Command::effect(Effect::SaveUser)
//!         }
//!     }
//! }
//! ```
//!
//! ### ⚡ **Async Effects with Resources**
//! ```rust
//! # use std::sync::Arc;
//! # use syzygy::executor::{InlineAsync, Task, TokioExecutor};
//! # use syzygy::prelude::*;
//! # struct Database;
//! # impl Database {
//! #     async fn save(&self) {}
//! # }
//! #[derive(Clone)]
//! struct Services {
//!     db: Arc<Database>,
//! }
//!
//! fn effects(effect: Effect, services: Services) -> Task<Event, Effect> {
//!     match effect {
//!         Effect::SaveUser => {
//!             let db = Arc::clone(&services.db);
//!             Task::async_on::<TokioExecutor, _>(async move {
//!                 db.save().await;
//!                 Command::event(Event::UserSaved)
//!             })
//!         }
//!         Effect::Log(msg) => Task::async_on::<InlineAsync<Event>, _>(async move {
//!             println!("LOG {msg}");
//!             Command::none()
//!         }),
//!     }
//! }
//! ```
//!
//! ## Runtime Support
//!
//! Syzygy ships with dedicated executors:
//!
//! - `TokioExecutor` – spawn async work onto a Tokio runtime you control
//! - `InlineAsync` – execute futures immediately on the caller thread (great for tests)
//! - `SingleThreadExecutor` – sequential, borrowing access to a worker resource
//! - `RayonExecutor` (optional feature) – CPU-heavy parallel work
//!
//! Bring additional runtimes by implementing the `AsyncOwnedExecutor` trait.
//!
//! ## Examples
//!
//! Learn Syzygy progressively with our example series:
//!
//! - **[basic_counter.rs]** – the smallest possible Syzygy app
//! - **[async_effect.rs]** – scheduling work onto Tokio executors
//! - **[manual_loop.rs]** – driving `Core`/`Shell` without the runner helper
//! - **[two_executors.rs]** – mixing IO and CPU executors under Tokio
//!
//! [basic_counter.rs]: https://github.com/ribelo/syzygy/blob/main/examples/basic_counter.rs
//! [async_effect.rs]: https://github.com/ribelo/syzygy/blob/main/examples/async_effect.rs
//! [manual_loop.rs]: https://github.com/ribelo/syzygy/blob/main/examples/manual_loop.rs
//! [two_executors.rs]: https://github.com/ribelo/syzygy/blob/main/examples/two_executors.rs
//!
//! ## Performance Benchmarks
//!
//! Syzygy delivers exceptional performance with real-world patterns:
//!
//! - **Task Spawning**: ~4ns (24x faster than alternatives)
//! - **Model Access**: ~0.31ns (single model), ~4.05ns (16 models)
//! - **Event Processing**: <100ns typical
//! - **Command Creation**: <50ns typical
//!
//! Run benchmarks yourself:
//! ```bash
//! cargo bench
//! ```
//!
//! ## The TEA Pattern
//!
//! The Elm Architecture provides predictable state management:
//!
//! ```text
//! ┌─────────────┐    Events    ┌──────────────┐    Commands    ┌─────────────┐
//! │    View     │──────────────►│    Update    │───────────────►│   Effects   │
//! │   (Your     │               │  (Pure Fn)   │                │ (Async Side │
//! │    App)     │               │              │                │   Effects)  │
//! └─────────────┘               └──────────────┘                └─────────────┘
//!       ▲                              │                              │
//!       │                              ▼                              │
//!       │                       ┌──────────────┐                      │
//!       │          New State    │    Model     │          Events      │
//!       └───────────────────────│   (State)    │◀─────────────────────┘
//!                               └──────────────┘
//! ```
//!
//! **Key Principles:**
//! - **Unidirectional Data Flow**: Events → Update → Model → Effects
//! - **Pure Updates**: No side effects in update functions
//! - **Predictable**: Same event always produces same state change
//! - **Composable**: Models, effects, and handlers compose cleanly
//!
//! ## Error Handling
//!
//! Syzygy follows the **"error-as-events"** pattern - all errors flow through
//! the same event pipeline for consistent handling:
//!
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default)] struct Model;
//! # #[derive(Debug, Clone)] enum Effect { SaveData }
//! #[derive(Debug, Clone)]
//! enum AppEvent {
//!     ProcessData { data: String },
//!     ValidationError { message: String },
//!     DataSaved,
//! }
//!
//! fn update(event: AppEvent, model: &mut Model) -> Command<AppEvent, Effect> {
//!     match event {
//!         AppEvent::ProcessData { data } => {
//!             if data.is_empty() {
//!                 // Error as event - consistent handling
//!                 Command::event(AppEvent::ValidationError {
//!                     message: "Data cannot be empty".to_string()
//!                 })
//!             } else {
//!                 // Success path
//!                 Command::effect(Effect::SaveData)
//!             }
//!         }
//!         AppEvent::ValidationError { message } => {
//!             eprintln!("Validation error: {}", message);
//!             Command::none()
//!         }
//!         AppEvent::DataSaved => {
//!             println!("Data saved successfully!");
//!             Command::none()
//!         }
//!     }
//! }
//! ```
//!
//! ## Architecture Overview
//!
//! Syzygy implements a **Core/Shell** architecture for clean separation of concerns:
//!
//! ### Core (Synchronous)
//! - Owns application state (models)
//! - Processes events through pure update functions
//! - Generates commands for side effects
//! - Deterministic and easily testable
//!
//! ### Shell (Asynchronous)
//! - Handles side effects (HTTP, database, file I/O)
//! - Manages resources (database pools, HTTP clients)
//! - Can send events back to Core
//! - Provides safe task spawning with automatic cleanup
//!
//! ### Runner (Orchestration)
//! - Coordinates between Core and Shell
//! - Provides simple tick-based execution model
//! - Handles event routing and command execution
//!
//! ## Advanced Patterns
//!
//! ### Multi-Model Access
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default)] struct UserModel { name: String }
//! # #[derive(Debug, Default)] struct ConfigModel { theme: String }
//! # #[derive(Debug, Default)] struct SessionModel { active: bool }
//! # type MyModel = (SessionModel, ConfigModel, UserModel);
//! # let model: MyModel = (SessionModel::default(), ConfigModel::default(), UserModel::default());
//! // Access individual models by type
//! let config: &ConfigModel = storage.get();
//! let session: &SessionModel = storage.get();
//! ```
//!
//! ### Background Task Management
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Clone)] enum Event { TaskComplete }
//! # #[derive(Debug, Clone)] enum MyEffect { DoWork }
//! async fn handle_effect(
//!     effect: MyEffect,
//!     _ctx: EffectContext<Event>,
//! ) -> Outcome<Event> {
//!     match effect {
//!         MyEffect::DoWork => {
//!             // Do async work and return result as event
//!             tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//!             Outcome::Event(Event::TaskComplete)
//!         }
//!     }
//! }
//! ```
//!
//! ## Safety Guarantees
//!
//! - **Memory Safety**: All spawned tasks automatically cancelled on context drop
//! - **Type Safety**: Compile-time verification of model and resource access
//! - **Concurrency Safety**: Interior mutability handled safely with `UnsafeCell`
//! - **Resource Safety**: No resource leaks or orphaned tasks
//!
//! ## Testing
//!
//! Syzygy applications are highly testable due to pure update functions:
//!
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default, PartialEq)] struct CounterModel { count: i32 }
//! # #[derive(Debug, Clone)] enum CounterEvent { Increment }
//! # #[derive(Debug, Clone)] enum CounterEffect { Log }
//! # fn update(event: CounterEvent, ctx: &mut EventContext<CounterEvent, CounterEffect, CounterModel>) -> Command<CounterEvent, CounterEffect> {
//! #     let model: &mut CounterModel = ctx.model_mut();
//! #     match event {
//! #         CounterEvent::Increment => { model.count += 1; Command::effect(CounterEffect::Log) }
//! #     }
//! # }
//! #[cfg(test)]
//! mod tests {
//!     use super::*;
//!
//!     #[test]
//!     fn test_counter_increment() {
//!         let mut model = CounterModel::default();
//!         let mut ctx = EventContext::new(&mut model);
//!
//!         let command = update(CounterEvent::Increment, &mut ctx);
//!
//!         let model: &CounterModel = &model;
//!         assert_eq!(model.count, 1);
//!
//!         // Verify command contains expected effect
//!         let effects: Vec<_> = command.into_iter()
//!             .filter_map(|step| match step {
//!                 CommandStep::Effect(effect) => Some(effect),
//!                 _ => None,
//!             })
//!             .collect();
//!         assert_eq!(effects.len(), 1);
//!     }
//! }
//! ```

// Core modules
pub mod command;
pub mod core;
// effect_context removed from public API; routing is handled internally
pub mod shell;
pub mod syzygy;

// Shared runtime facade powering scheduling, spawning, and timers
// Builder pattern
pub mod builder;

// EventContext for synchronous update functions
// event_context removed from public API; event handlers receive &mut model directly

// Error handling
pub mod error;

// Storage system with UnsafeCell-based chains
// Storage module removed - using direct FxHashMap for resources

// Executor system for specialized effect handling
pub mod executor;
pub mod resource_cell;

pub mod prelude {
    // Contexts for update and effect functions
    // (removed) EffectContext and EventContext were deleted; handlers receive plain args

    // Command system
    pub use crate::command::{Command, CommandStep, IntoCommand};

    // Core/Shell architecture
    pub use crate::core::{Core, EventHandler};
    pub use crate::shell::Shell;
    pub use crate::syzygy::{Syzygy, SyzygyConfig};

    // Type aliases for common use cases
    /// A simple Shell for applications that only need models (no resources or executors).
    /// This is the most common case for basic applications.
    ///
    /// # Example
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// #[derive(Debug, Clone)]
    /// enum Event { Increment }
    /// #[derive(Debug, Clone)]
    /// enum Effect { Log }
    ///
    /// type MyModel = ();
    /// type MyCore = Core<Event, Effect, MyModel>;
    /// type MyShell = Shell<Event, Effect>;
    ///
    /// fn build() -> (MyCore, MyShell) {
    ///     Syzygy::builder()
    ///         .model(())  // Some model
    ///         .event_handler(|_event, _ctx| Command::none())
    ///         .effect_handler(|_effect, _ctx| Task::none())
    ///         .build()
    /// }
    /// ```
    // Effect handlers with AFIT
    pub use crate::shell::EffectHandler;

    // Executor system
    #[cfg(feature = "rayon")]
    pub use crate::executor::RayonExecutor;

    #[cfg(feature = "tokio")]
    pub use crate::executor::TokioExecutor;
    pub use crate::executor::{ExecutorRegistry, InlineAsync, SingleThreadExecutor, Task};

    // Builder
    pub use crate::builder::SyzygyBuilder;

    // Errors
    pub use crate::error::{CommandError, CoreError, EffectError, ShellError};

    // Effect output types

    pub use crate::resource_cell::{ResourceCell, SetOnceError};
    // spawn facade removed; use tokio::runtime::Handle directly when needed
}
