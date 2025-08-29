//! # Syzygy - Zero-Overhead TEA for Rust
//!
//! A high-performance implementation of The Elm Architecture (TEA) with Core/Shell separation,
//! providing deterministic state management with async side effects and multi-model composition.
//!
//! ## Quick Start
//!
//! ```rust
//! use syzygy::prelude::*;
//!
//! // 1. Define your models
//! #[derive(Debug, Default)]
//! struct CounterModel {
//!     count: i32,
//! }
//!
//! // 2. Define events and effects
//! #[derive(Debug, Clone)]
//! enum CounterEvent {
//!     Increment,
//!     Decrement,
//! }
//!
//! #[derive(Debug, Clone)]
//! enum CounterEffect {
//!     LogMessage(String),
//! }
//!
//! // 3. Write your update function (the heart of TEA)
//! fn update_counter(
//!     event: CounterEvent,
//!     ctx: &mut EventContext<CounterEvent, CounterEffect, Storage<CounterModel, EmptyStorage>>,
//! ) -> Command<CounterEvent, CounterEffect> {
//!     let model: &mut CounterModel = ctx.model_mut();
//!
//!     match event {
//!         CounterEvent::Increment => {
//!             model.count += 1;
//!             Command::effect(CounterEffect::LogMessage(
//!                 format!("Count incremented to {}", model.count)
//!             ))
//!         }
//!         CounterEvent::Decrement => {
//!             model.count -= 1;
//!             Command::effect(CounterEffect::LogMessage(
//!                 format!("Count decremented to {}", model.count)
//!             ))
//!         }
//!     }
//! }
//!
//! // 4. Handle side effects
//! async fn handle_effects(
//!     effect: CounterEffect,
//!     _ctx: EffectContext<CounterEvent, EmptyStorage>
//! ) {
//!     match effect {
//!         CounterEffect::LogMessage(message) => {
//!             println!("LOG: {}", message);
//!         }
//!     }
//! }
//!
//! // 5. Build and run your application
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let (core, shell) = Syzygy::builder()
//!     .model(CounterModel::default())
//!     .event_handler(update_counter)
//!     .effect_handler(handle_effects)
//!     .build();
//! let mut runner = Runner::new(core, shell);
//!
//! // Send events and run
//! runner.core().send_event(CounterEvent::Increment)?;
//! runner.tick(syzygy::spawn::spawner()).await?;
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
//! # fn update(e: Event, ctx: &mut EventContext<Event, Effect, Storage<ConfigModel, Storage<UserModel, EmptyStorage>>>) -> Command<Event, Effect> { Command::none() }
//! let (core, shell) = Syzygy::builder()
//!     .model(UserModel::default())     // Add multiple models
//!     .model(ConfigModel::default())   // Type-safe composition
//!     .event_handler(update)
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
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Clone)] struct Database { url: String }
//! # #[derive(Debug, Clone)] enum Event { UserSaved }
//! # #[derive(Debug, Clone)] enum Effect { SaveUser }
//! async fn handle_effects(
//!     effect: Effect,
//!     database: &Database,           // Extract resources
//!     sender: EventSender<Event>,    // Send events back
//! ) {
//!     match effect {
//!         Effect::SaveUser => {
//!             println!("Saving to {}", database.url);
//!             // Perform async work...
//!             let _ = sender.send(Event::UserSaved);
//!         }
//!     }
//! }
//! ```
//!
//! ## Runtime Support
//!
//! **Tokio-first with runtime flexibility:**
//!
//! - **🥇 Tokio** (Default) - Production recommended
//! - **🥈 Smol** - Lightweight for embedded/WASM
//! - **🥉 Async-std** - Standard library approach
//! - **🔧 Custom** - Bring your own executor
//!
//! ```toml
//! [dependencies]
//! # Default: tokio
//! syzygy = "0.1"
//!
//! # Alternative runtimes
//! syzygy = { version = "0.1", default-features = false, features = ["smol"] }
//! syzygy = { version = "0.1", default-features = false, features = ["async-std"] }
//! ```
//!
//! ## Examples
//!
//! Learn Syzygy progressively with our example series:
//!
//! - **[01_basic_tea.rs]** - Core TEA patterns and concepts
//! - **[02_multi_model.rs]** - Working with multiple models
//! - **[03_magic_handlers.rs]** - Automatic parameter extraction
//! - **[04_async_effects.rs]** - Resources and async effects
//! - **[05_real_world_app.rs]** - Complete production patterns
//!
//! [01_basic_tea.rs]: https://github.com/ribelo/syzygy/blob/main/examples/01_basic_tea.rs
//! [02_multi_model.rs]: https://github.com/ribelo/syzygy/blob/main/examples/02_multi_model.rs
//! [03_magic_handlers.rs]: https://github.com/ribelo/syzygy/blob/main/examples/03_magic_handlers.rs
//! [04_async_effects.rs]: https://github.com/ribelo/syzygy/blob/main/examples/04_async_effects.rs
//! [05_real_world_app.rs]: https://github.com/ribelo/syzygy/blob/main/examples/05_real_world_app.rs
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
//! fn update(
//!     event: AppEvent,
//!     ctx: &mut EventContext<AppEvent, Effect, Storage<Model, EmptyStorage>>,
//! ) -> Command<AppEvent, Effect> {
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
//! ### Bulk Model Extraction
//! ```rust
//! # use syzygy::prelude::*;
//! # use syzygy::storage::BulkExtract;
//! # #[derive(Debug, Default)] struct UserModel { name: String }
//! # #[derive(Debug, Default)] struct ConfigModel { theme: String }
//! # #[derive(Debug, Default)] struct SessionModel { active: bool }
//! # type MyStorage = Storage<SessionModel, Storage<ConfigModel, Storage<UserModel, EmptyStorage>>>;
//! # let storage: MyStorage = EmptyStorage.with_model(UserModel::default()).with_model(ConfigModel::default()).with_model(SessionModel::default());
//! // Extract multiple models efficiently (30-41% faster than individual calls)
//! let (user, config, session): (&UserModel, &ConfigModel, &SessionModel) =
//!     storage.extract_bulk();
//! ```
//!
//! ### Resource Management
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default)] struct MyModel;
//! # #[derive(Clone)] struct Database { url: String }
//! # #[derive(Clone)] struct HttpClient { base_url: String }
//! # #[derive(Debug, Clone)] enum Event { Test }
//! # #[derive(Debug, Clone)] enum Effect { Test }
//! # fn update(e: Event, ctx: &mut EventContext<Event, Effect, Storage<MyModel, EmptyStorage>>) -> Command<Event, Effect> { Command::none() }
//! let (core, shell) = Syzygy::builder()
//!     .model(MyModel::default())
//!     .resource(Database { url: "postgres://...".to_string() })
//!     .resource(HttpClient { base_url: "https://api.example.com".to_string() })
//!     .event_handler(update)
//!     .build();
//! ```
//!
//! ### Background Task Management
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Clone)] enum Event { TaskComplete }
//! # #[derive(Debug, Clone)] enum MyEffect { DoWork }
//! async fn handle_effect(
//!     effect: MyEffect,
//!     ctx: EffectContext<Event, EmptyStorage>,
//! ) {
//!     match effect {
//!         MyEffect::DoWork => {
//!             // All spawned tasks automatically cancelled when context drops
//!             let ctx_clone = ctx.clone();
//!             ctx.spawn(async move {
//!                 // Long running background work
//!                 tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//!                 let _ = ctx_clone.send_event(Event::TaskComplete);
//!             }).unwrap();
//!
//!             // Spawn multiple tasks safely
//!             for i in 0..10 {
//!                 let ctx_clone = ctx.clone();
//!                 ctx.spawn(async move {
//!                     println!("Background task {}", i);
//!                 }).unwrap();
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! ## Safety Guarantees
//!
//! - **Memory Safety**: All spawned tasks automatically cancelled on context drop
//! - **Type Safety**: Compile-time verification of model and resource access
//! - **Concurrency Safety**: Interior mutability handled safely with UnsafeCell
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
//! # fn update(event: CounterEvent, ctx: &mut EventContext<CounterEvent, CounterEffect, Storage<CounterModel, EmptyStorage>>) -> Command<CounterEvent, CounterEffect> {
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
//!         let mut storage = EmptyStorage.with_model(CounterModel::default());
//!         let mut ctx = EventContext::new(&mut storage);
//!
//!         let command = update(CounterEvent::Increment, &mut ctx);
//!
//!         let model: &CounterModel = storage.get();
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

// Executor system for specialized effect handling
pub mod executor;

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
    pub use crate::core::{Core, EventHandler};
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
    pub use crate::storage::{Contains, EmptyStorage, Storage};

    // Executor system
    pub use crate::executor::{SpawnExecutor, ExecutorStorage, EmptyExecutorStorage, TokioExecutor};

    // Magic handler system
    pub use crate::extract::{EventSender, FromEffectContext, FromEventContext};
    pub use crate::magic_handler::{
        EffectMagicHandler, EventMagicHandler, UnitHandler, event_trigger,
    };

    // Magic handler macros
    pub use crate::effect_magic_handler;
    pub use crate::event_magic_handler;

    // Derive macros
    pub use syzygy_macros::MagicVariants;

    // Builder
    pub use crate::builder::{Syzygy, SyzygyBuilder};

    // Errors
    pub use crate::error::{CommandError, CoreError, ShellError};
}
