//! # Syzygy - Zero-Overhead TEA for Rust
//!
//! A high-performance implementation of The Elm Architecture (TEA) with Core/Shell separation,
//! providing deterministic state management with async side effects and a single root model.
//!
//! ## Quick Start
//!
//! ```rust
//! use syzygy::executor::{InlineAsync, Task};
//! use syzygy::prelude::*;
//!
//! #[derive(Default)]
//! struct User {
//!     name: String,
//! }
//!
//! #[derive(Default)]
//! struct Counter {
//!     value: i32,
//! }
//!
//! #[derive(Default)]
//! struct App {
//!     user: User,
//!     counter: Counter,
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
//! fn event_handler(
//!     event: CounterEvent,
//!     model: &mut App,
//! ) -> Command<CounterEvent, CounterEffect> {
//!     match event {
//!         CounterEvent::Increment => {
//!             model.counter.value += 1;
//!             Command::effect(CounterEffect::Log(format!("{}", model.counter.value)))
//!         }
//!         CounterEvent::Decrement => {
//!             model.counter.value -= 1;
//!             Command::effect(CounterEffect::Log(format!("{}", model.counter.value)))
//!         }
//!     }
//! }
//!
//! fn effect_handler(effect: CounterEffect, resources: &'static str) -> Task<CounterEvent, CounterEffect> {
//!     match effect {
//!         CounterEffect::Log(message) => Task::async_on::<InlineAsync, _>(async move {
//!             println!("{resources} {message}");
//!             Command::none()
//!         }),
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
//!     .model(App::default())
//!     .with_resources("LOG")
//!     .event_handler(event_handler)
//!     .effect_handler(effect_handler)
//!     .with_async_executor(InlineAsync::new())
//!     .build();
//!
//! runner.core().try_send_event(CounterEvent::Increment)?;
//! runner.run_until(|core, _shell| core.model().counter.value == 1)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Core Features
//!
//! ### Zero-Overhead Performance
//! - Direct storage access without runtime overhead
//! - Compile-time type safety with zero-cost abstractions
//! - Task spawning 24x faster than alternatives (~4ns per task)
//!
//! ### Single Root Model
//! Compose your own application state and register it once on the builder:
//! ```rust
//! # use syzygy::executor::Task;
//! # use syzygy::prelude::*;
//! #[derive(Default)]
//! struct UserModel { name: String }
//! #[derive(Default)]
//! struct ConfigModel { theme: String }
//! #[derive(Default)]
//! struct AppModel { user: UserModel, config: ConfigModel }
//! # #[derive(Clone)] enum Event { Test }
//! # #[derive(Clone)] enum Effect { Test }
//! # fn update(_event: Event, _model: &mut AppModel) -> Command<Event, Effect> { Command::none() }
//! # fn effects(_effect: Effect, _resources: ()) -> Task<Event, Effect> { Task::none() }
//! let runner = Syzygy::builder::<Event, Effect>()
//!     .model(AppModel::default())
//!     .event_handler(update)
//!     .effect_handler(effects)
//!     .build();
//! # let _ = runner;
//! ```
//!
//! ### Async Effects with Resources
//! ```rust
//! # use std::sync::Arc;
//! # use syzygy::executor::{InlineAsync, Task};
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
//!             Task::async_on::<InlineAsync, _>(async move {
//!                 db.save().await;
//!                 Command::event(Event::UserSaved)
//!             })
//!         }
//!         Effect::Log(msg) => Task::async_on::<InlineAsync, _>(async move {
//!             println!("LOG {msg}");
//!             Command::none()
//!         }),
//!     }
//! }
//! ```
//!
//! ## Runtime Support
//!
//! Syzygy ships with a minimal inline executor out of the box.
//!
//! - `InlineAsync` – deterministic inline executor for tests and CLIs
//! - Additional executors live in companion crates such as `syzygy-executor-tokio`,
//!   `syzygy-executor-single`, and `syzygy-executor-rayon`
//!
//! Bring additional runtimes by implementing the `AsyncExecutor` trait or by
//! depending on the companion crates listed above.
//!
//! ## Examples
//!
//! Learn Syzygy progressively with our example series:
//!
//! - **[basic_counter.rs]** – the smallest possible Syzygy app
//! - **[async_effect.rs]** – scheduling work onto Tokio executors
//! - **[timeout_pattern.rs]** – modeling timeouts as explicit events with retries
//! - **[manual_loop.rs]** – driving `Core`/`Shell` without the runner helper
//! - **[two_executors.rs]** – mixing IO and CPU executors under Tokio
//!
//! [basic_counter.rs]: https://github.com/ribelo/syzygy/blob/main/examples/basic_counter.rs
//! [async_effect.rs]: https://github.com/ribelo/syzygy/blob/main/examples/async_effect.rs
//! [manual_loop.rs]: https://github.com/ribelo/syzygy/blob/main/examples/manual_loop.rs
//! [timeout_pattern.rs]: https://github.com/ribelo/syzygy/blob/main/examples/timeout_pattern.rs
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
//!         AppEvent::ValidationError { message } => Command::none(),
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
//! # #[derive(Debug, Default)] struct UserModel { name: String }
//! # #[derive(Debug, Default)] struct ConfigModel { theme: String }
//! # #[derive(Debug, Default)] struct SessionModel { active: bool }
//! # let mut model = (SessionModel::default(), ConfigModel::default(), UserModel::default());
//! // Model is your own type; pattern match to access parts
//! let (session, config, user) = &model;
//! assert!(!session.active);
//! let _ = &config.theme;
//! let _ = &user.name;
//! ```
//!
//! ### Background Task Management
//! ```rust
//! # use syzygy::prelude::*;
//! # use syzygy::executor::{Task, InlineAsync};
//! # #[derive(Debug, Clone)] enum Event { TaskComplete }
//! # #[derive(Debug, Clone)] enum MyEffect { DoWork }
//! fn handle_effect(effect: MyEffect, _res: ()) -> Task<Event, MyEffect> {
//!     match effect {
//!         MyEffect::DoWork => Task::async_on::<InlineAsync, _>(async move {
//!             // Do async work and return events/effects via Command
//!             // tokio timers require a runtime; InlineAsync uses thread sleep
//!             Command::event(Event::TaskComplete)
//!         }),
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
//! # #[derive(Debug, Clone, PartialEq)] enum CounterEffect { Log }
//! fn update(event: CounterEvent, model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
//!     match event {
//!         CounterEvent::Increment => { model.count += 1; Command::effect(CounterEffect::Log) }
//!     }
//! }
//! # #[cfg(test)]
//! # mod tests { use super::*; #[test] fn test_counter_increment() {
//! #     let mut model = CounterModel::default();
//! #     let command = update(CounterEvent::Increment, &mut model);
//! #     assert_eq!(model.count, 1);
//! #     let steps: Vec<_> = command.into_iter().collect();
//! #     assert!(matches!(steps[0], CommandStep::Effect(CounterEffect::Log)));
//! # }}
//! ```

// Core modules
pub mod activity;
pub mod command;
pub mod core;
// EffectContext/EventContext were removed from public API; handlers receive
// plain arguments: event handlers get `&mut model`, effect handlers get
// `(effect, resources)` and return `Task`.
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "shell")]
pub mod syzygy;

// Shared runtime facade powering scheduling, spawning, and timers
// Builder pattern
#[cfg(feature = "shell")]
pub mod builder;

// EventContext removed from public API; event handlers receive &mut model directly

// Error handling
pub mod error;

// Resources are provided by user code and cloned per-effect; no internal storage module.

// Executor system for specialized effect handling
#[cfg(feature = "shell")]
pub mod executor;
pub mod resource_cell;

pub mod prelude {
    // No public contexts in the new API; handlers receive plain args

    // Command system
    pub use crate::command::builders as command;
    pub use crate::command::builders as cmd;
    pub use crate::command::builders::{
        batch, effect, effects, event, events, none, parallel, sequential,
    };
    pub use crate::command::{Command, CommandStep};

    // Core/Shell architecture
    pub use crate::activity::Activity;
    pub use crate::core::{Core, EventHandler, EventSender};
    #[cfg(feature = "shell")]
    pub use crate::shell::{Shell, ShellStats, ShellStatsSnapshot};
    #[cfg(feature = "shell")]
    pub use crate::syzygy::{Runner, Syzygy, SyzygyConfig};

    // Type aliases for common use cases
    /// A simple Shell for applications that only need models (no resources or executors).
    /// This is the most common case for basic applications.
    ///
    /// # Example
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// #[derive(Debug, Clone)]
    /// enum Event {
    ///     Increment,
    /// }
    /// #[derive(Debug, Clone)]
    /// enum Effect {
    ///     LogTick,
    /// }
    ///
    /// type MyModel = u32;
    /// type MyCore = Core<Event, Effect, MyModel>;
    /// type MyShell = Shell<Event, Effect>;
    ///
    /// fn build() -> (MyCore, MyShell) {
    ///     Syzygy::builder::<Event, Effect>()
    ///         .model(0u32)
    ///         .event_handler(|event, model| match event {
    ///             Event::Increment => {
    ///                 *model += 1;
    ///                 cmd::effect(Effect::LogTick)
    ///             }
    ///         })
    ///         .effect_handler(|effect, _| match effect {
    ///             Effect::LogTick => Command::none(),
    ///         })
    ///         .build()
    /// }
    /// ```
    // Effect handlers with AFIT
    #[cfg(feature = "shell")]
    pub use crate::shell::EffectHandler;

    // Executor system
    #[cfg(all(feature = "shell", feature = "rayon"))]
    pub use crate::executor::RayonExecutor;

    #[cfg(feature = "shell")]
    pub use crate::executor::{
        ExecutorRegistry, InlineAsync, PanicDetails, PanicHook, PanicTaskKind, Plan, Task,
    };

    // Builder
    #[cfg(feature = "shell")]
    pub use crate::builder::SyzygyBuilder;

    // Errors
    #[cfg(feature = "shell")]
    pub use crate::error::ShellError;
    pub use crate::error::{CommandError, CoreError, EffectError};

    // Effect output types

    pub use crate::resource_cell::{ResourceCell, SetOnceError};
    // spawn facade removed; use tokio::runtime::Handle directly when needed
}
