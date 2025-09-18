# Syzygy

**Elm for everything that isn't a web server** - A zero-overhead state management library for Rust targeting event-driven applications with predictable state evolution.

Perfect for desktop apps, game servers, CLI tools, IoT systems, and simple servers where events must be processed in order.

## Why Syzygy?

Most software isn't web servers - it's desktop apps, games, CLI tools, IoT devices, and simple servers. These applications share common needs:
- **Predictable state evolution** - events processed in order, deterministic outcomes  
- **Strong compile-time guarantees** - prevent bugs before they happen
- **Testable architecture** - separate pure logic from side effects
- **Resource management** - proper cleanup of files, connections, background tasks

## Features

- 🎯 **The Elm Architecture** - Unidirectional data flow (Event → Model → Command)
- 🔧 **Core/Shell Separation** - Pure sync Core + async Shell for effects  
- 📋 **Predictable Event Processing** - FIFO ordering, deterministic behavior
- 🧪 **Magic Handlers** - Axum-inspired parameter extraction for testable code
- 🚀 **Zero-overhead Commands** - Simple data structures, no complex execution
- 🌊 **User-defined effects** - Library provides no effects, users define their own
- ⚡ **Sequential & Parallel execution** - Predictable, composable command processing
- 🛡️ **Error-as-events** - All errors flow through the same event pipeline
- 🏎️ **High Performance** - Handles 100K+ events/sec with safety guarantees

## Quick Start

```rust
use std::time::Duration;

use syzygy::executor::{ExecutorRegistry, InlineAsync, Outcome, Task, TokioExecutor};
use syzygy::prelude::*;

// Define your events (what can happen)
#[derive(Debug, Clone)]
enum AppEvent {
    Increment,
    LoadData,
    DataLoaded { data: String },
    Error { message: String },
}

// Define your model (application state)
#[derive(Debug, Default)]
struct AppModel {
    counter: i32,
    data: Option<String>,
    is_loading: bool,
}

// Define your effects (what you want to do)
#[derive(Debug, Clone)]
enum AppEffect {
    HttpRequest { url: String },
    Log { message: String },
}

// Event handler function (no trait needed!)
fn event_handler(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppModel>,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Increment => on_increment(ctx.model_mut()),
        AppEvent::LoadData => on_load_data(ctx.model_mut()),
        AppEvent::DataLoaded { data } => on_data_loaded(ctx.model_mut(), data),
        AppEvent::Error { message } => on_error(ctx.model_mut(), message),
    }
}

fn on_increment(model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    model.counter += 1;
    Command::effect(AppEffect::Log {
        message: format!("Counter: {}", model.counter),
    })
}

fn on_load_data(model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    model.is_loading = true;
    Command::parallel([
        AppEffect::HttpRequest {
            url: "https://api.example.com/data".to_string(),
        },
        AppEffect::Log {
            message: "Loading data…".to_string(),
        },
    ])
}

fn on_data_loaded(model: &mut AppModel, data: String) -> Command<AppEvent, AppEffect> {
    model.data = Some(data);
    model.is_loading = false;
    Command::effect(AppEffect::Log {
        message: "Loaded data successfully".to_string(),
    })
}

fn on_error(model: &mut AppModel, message: String) -> Command<AppEvent, AppEffect> {
    model.is_loading = false;
    Command::effect(AppEffect::Log { message })
}

fn effect_handler(
    effect: AppEffect,
    _ctx: EffectContext<AppEvent, ()>,
) -> Task<AppEvent, ()> {
    match effect {
        AppEffect::HttpRequest { url } => fetch_data(url),
        AppEffect::Log { message } => log_message(message),
    }
}

fn fetch_data(url: String) -> Task<AppEvent, ()> {
    Task::async_task::<TokioExecutor, _>(async move {
        println!("🌐 Fetching: {url}");
        tokio::time::sleep(Duration::from_millis(100)).await;
        Outcome::Event(AppEvent::DataLoaded {
            data: "Hello from API!".to_string(),
        })
    })
}

fn log_message(message: String) -> Task<AppEvent, ()> {
    Task::async_task::<InlineAsync<AppEvent>, _>(async move {
        println!("📝 {message}");
        Outcome::None
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .with_async_executor(InlineAsync::<AppEvent>::new())
        .with_async_executor(TokioExecutor::multi_thread_io("app-io", 2))
        .build();

    app.core().send_event(AppEvent::Increment)?;
    app.core().send_event(AppEvent::LoadData)?;

    app.run_until(|core, _| !core.model().is_loading)?;

    println!("Final state: {:?}", app.core().model());
    Ok(())
}
```

## Perfect For

**✅ Desktop Applications**
- GUI apps (egui, dioxus, tauri) with complex state
- Note-taking apps, IDEs, media players
- Configuration management tools

**✅ Game Development** 
- Turn-based games with complex state machines
- Real-time games with centralized state
- Game servers and matchmaking systems

**✅ System Tools**
- CLI tools with interactive modes
- Build systems and deployment tools
- IoT device controllers and data processors

**✅ Simple Servers**
- Chat servers and notification systems  
- Real-time data processing pipelines
- Workflow engines and task orchestrators
- Financial systems where event ordering matters

**❌ Not Ideal For**
- High-concurrency web servers (use axum/warp instead)
- Distributed systems with multiple nodes
- Applications where massive parallelism is core requirement

## Architecture

Syzygy follows **The Elm Architecture** (TEA) with a strict, unidirectional flow and a clear split between pure state changes and impure effects.

```
External World              Core (Pure)                    Shell (Impure)
 ─────────────     ┌────────────────────────┐    ┌──────────────────────────┐
  User, IO, etc ──►│   update(event,model)  │───►│ Execute Command outputs  │
                   │   ┌──────────────────┐  │    │  - Route events to Core │
                   │   │   Command<Event, │  │    │  - Route effects to     │
                   │   │         Effect>  │  │    │    Effect Handler       │
                   │   └──────────────────┘  │    └─────────────┬──────────┘
                   └────────────────────────┘                  │ Effects
                                                               ▼
                                                        Effect Handler (async)
                                                        ┌────────────────────┐
                                                        │ ctx.run_on::<Exec> │
                                                        │ per-effect routing │
                                                        └──────────┬─────────┘
                                                                   │ spawns
                            Multiple Executors (Policy)            ▼
     ┌────────────────────────────┬────────────────────────────┬───────────────┐
     │ TokioExecutor              │ ThreadPerCoreTokioExecutor │ SingleThread │
     │ (general async IO)         │ (actix-like per-core)      │ (single-writer)
     ├────────────────────────────┼────────────────────────────┼───────────────┤
     │ RayonExecutor (CPU heavy, pure compute)                  │ ... custom   │
     └──────────────────────────────────────────────────────────┴──────────────┘

Event → Core.event_handler() → Command → Shell.execute() → Effect Handler → Executor → Event
```

### Core (Pure)
- Manages application state synchronously
- Processes events in FIFO order (no surprises at 3 AM)
- Event handlers are pure functions - no I/O, no async
- Returns Commands that tell the Shell what to do
- Zero runtime overhead - just a state machine with benefits

### Shell (Impure)
- Executes Commands from Core
- Handles all async operations and side effects
- Manages executor registry for different work types
- Routes events back to Core when effects complete
- Your async playground - but with adult supervision

### Executors (Runtime Services)
Syzygy uses a specialized two-trait executor system that eliminates impedance mismatches between async and sync work.

#### Built-in Executors
- **`TokioExecutor`**: General async work (network, files) with `enable_all()` runtime
- **`RayonSyncExecutor`**: Parallel CPU work using Rayon's work-stealing
- **`SingleThreadExecutor`**: Sequential sync work with strict FIFO ordering
- **`ThreadPerCoreTokioExecutor`**: Thread-per-core Tokio for CPU-bound async work

### Commands
Commands are the bridge between your pure event handler and the chaotic async world. They're like a shopping list for side effects - your event handler writes it, the Shell executes it.

```rust
// Single effect - runs when the Shell gets around to it
Command::effect(MyEffect::HttpRequest { url: "https://api.example.com".to_string() })

// Send event back to Core immediately
Command::event(MyEvent::DataLoaded { data: "hello".to_string() })

// Batch effects - run sequentially, no race conditions
Command::batch([
    Command::effect(MyEffect::Log { message: "Starting".to_string() }),
    Command::effect(MyEffect::HttpRequest { url: "https://api.example.com".to_string() }),
    Command::effect(MyEffect::Log { message: "Finished".to_string() }),
])

// Do nothing (useful for conditional logic)
Command::none()
```

#### Command Composition Patterns
Commands compose predictably - no magic, no surprises:

```rust
// Sequential effects - each waits for the previous
Command::batch([
    Command::effect(Effect::ValidateInput),
    Command::effect(Effect::SaveToDatabase),
    Command::effect(Effect::SendNotification),
])

// Parallel effects - use multiple Commands in a batch
Command::batch([
    Command::effect(Effect::FetchUserData),
    Command::effect(Effect::FetchProductData),
    Command::effect(Effect::FetchOrderData),
])
```

## Key Concepts

### Events
Events are the only way to change state in Syzygy. They represent everything that can happen in your application.

```rust
#[derive(Debug, Clone)]
enum MyEvent {
    UserClicked,
    DataLoaded { result: String },
    NetworkError { reason: String },
}
```

### Model
The model is your application state. It should be plain data structures - no methods, no async, just state.

```rust
#[derive(Debug, Default)]
struct MyModel {
    users: Vec<User>,
    is_loading: bool,
    error_message: Option<String>,
}
```

### Effects
Effects represent side effects you want to perform. They're just data - the library doesn't execute them directly.

```rust
#[derive(Debug, Clone)]
enum MyEffect {
    HttpGet { url: String },
    SaveToDatabase { data: String },
    ShowNotification { message: String },
}
```

### Error-as-Events
All errors flow through the same event pipeline. No special error handling, no exceptions, just events.

```rust
fn update(event: MyEvent, model: &mut MyModel) -> Command<MyEvent, MyEffect> {
    match event {
        MyEvent::DataLoaded { result } => {
            if result.is_empty() {
                // Error as event - no special handling needed
                return Command::event(MyEvent::NetworkError {
                    reason: "Empty response from server".to_string()
                });
            }
            // Success path
            model.data = Some(result);
            Command::none()
        }
        MyEvent::NetworkError { reason } => {
            // Handle error events like any other event
            model.error_message = Some(reason);
            model.is_loading = false;
            Command::none()
        }
    }
}
```

## Setup Options

### Auto-Wired (Default)
The builder handles all the wiring for you. This is what you want 95% of the time.

```rust
let (core, shell) = Syzygy::builder::<MyEvent, MyEffect>()
    .model(MyModel::default())
    .event_handler(my_update)
    .effect_handler(my_effect_handler)
    .build();
```

### Manual Wiring (Advanced Use Cases)
When you need to inject dependencies or customize the setup:

```rust
let (core, shell) = Syzygy::builder::<MyEvent, MyEffect>()
    .model(MyModel::default())
    .event_handler(my_update)
    .effect_handler(my_effect_handler)
    .with_async_executor(TokioExecutor::multi_thread_io("io-pool", 4))
    .with_async_executor(TokioExecutor::multi_thread_cpu("cpu-pool", 4))
    .with_sync_executor(RayonExecutor::new())
    .build();
```

## Performance

Syzygy is designed for high-performance event processing with deterministic behavior.

### Executor Performance
The specialized executor architecture eliminates impedance mismatches:

```rust
// IO-bound async work - ~4ns per task spawn
Task::async_task::<TokioExecutor, _>(async move {
    let response = reqwest::get("https://api.example.com").await?;
    // Network operations here
    Outcome::None
});

// CPU-bound async work - same performance
Task::async_task::<TokioExecutor, _>(async move {
    let result = expensive_async_computation().await;
    Outcome::None
});

// Parallel sync work - Rayon work-stealing
Task::Sync {
    exec: std::any::TypeId::of::<RayonExecutor>(),
    task: Box::new(|_| Outcome::None),
};

// Sequential sync work - strict FIFO
Task::Sync {
    exec: std::any::TypeId::of::<SingleThreadExecutor>(),
    task: Box::new(|_| Outcome::None),
};
```

## Runtime Support
Syzygy ships with production-ready executors so you can match every workload to the right engine:

- **InlineAsync** – deterministic single-threaded execution for CLIs and tests
- **TokioExecutor** – dedicated Tokio runtimes for async IO and CPU work
- **SingleThreadExecutor** – FIFO execution for blocking operations that must stay ordered
- **RayonExecutor** *(optional feature)* – parallel CPU work with Rayon

Register them directly on the builder:

```rust
Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(update)
    .effect_handler(effects)
    .with_async_executor(TokioExecutor::multi_thread_io("io", 4))
    .with_async_executor(TokioExecutor::multi_thread_cpu("cpu", 4))
    .with_sync_executor(RayonExecutor::new())
    .build();
```

Need something custom? Implement the unified `AsyncExecutor` trait, register it with `with_async_executor`, and Syzygy will drive it alongside the built-ins.

## Executor Architecture

Syzygy uses a specialized two-trait executor system for optimal performance:

### Executor Types
```rust
/// Shared lifecycle management for all executor types
pub trait ExecutorLifecycle: Send + Sync + 'static {
    fn shutdown(&self);
    fn join(&self) -> BoxFuture<'static, ()>;
}

/// Executor specialized for async work (futures)
pub trait AsyncExecutor<E>: ExecutorLifecycle {
    fn spawn_future(
        &self,
        fut: BoxFuture<'static, Outcome<E>>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>>;
}

/// Executor specialized for blocking/synchronous work  
pub trait SyncExecutor<E>: ExecutorLifecycle {
    fn spawn_sync(
        &self,
        job: Box<dyn FnOnce() -> Outcome<E> + Send>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>>;
}
```

### Configuration
The `ExecutorRegistry<E>` maintains separate registries for async and sync executors:

```rust
pub struct ExecutorRegistry<E> {
    async_map: FxHashMap<TypeId, Arc<dyn AsyncExecutor<E>>>,
    sync_map: FxHashMap<TypeId, Arc<dyn SyncExecutor<E>>>,
}

// Marker traits for type safety
pub trait AsyncKey: 'static {}  // For I/O-bound work  
pub trait SyncKey: 'static {}   // For CPU-bound work

// Pre-defined markers
pub struct Io;     // impl AsyncKey for Io  
pub struct Cpu;    // impl SyncKey for Cpu
```

### IO Runtime Registration
Syzygy provides IO runtime registration inspired by InfluxDB's design:

```rust
use syzygy::executor::{register_current_runtime_for_io, spawn_io};

// Register the current runtime for IO operations
register_current_runtime_for_io();

// Later, spawn IO work on the registered runtime
spawn_io(async {
    // Network/file IO operations here
    println!("Running on IO runtime");
});
```

This ensures IO operations run on the appropriate runtime while CPU-bound effects stay on their dedicated executors.

## Multi-Executor Routing

Syzygy supports routing effects to different executors based on workload type:

```rust
struct HttpClient;
struct Database;

struct NetExec;
struct DbExec;

async fn handle_effects(effect: MyEffect, ctx: EffectContext<MyEvent>) -> Task<MyEvent> {
    match effect {
        MyEffect::HttpGet { url } => {
            // Route to IO executor
            Task::async_task::<NetExec, _>(async move {
                let response = reqwest::get(&url).await?;
                Outcome::Event(MyEvent::DataLoaded { data: response.text().await? })
            })
        }
        MyEffect::SaveToDatabase { data } => {
            // Route to database executor
            Task::async_task::<DbExec, _>(async move {
                database.save(&data).await?;
                Outcome::Event(MyEvent::SaveComplete)
            })
        }
    }
}
```

## Installation

### Tokio (Recommended)
```toml
[dependencies]
syzygy = { git = "https://github.com/your-repo/syzygy" }
tokio = { version = "1.0", features = ["full"] }
```

## Documentation
- [API Documentation](https://docs.rs/syzygy)
- [Examples](./examples/)
- [Architecture Guide](./docs/architecture.md)

## License
MIT OR Apache-2.0

## Rationale

Syzygy exists because most state management solutions for Rust are either:
1. **Web-focused** (axum, warp, actix-web) - overkill for desktop apps
2. **Too complex** (full FRP systems) - learning curve steeper than Everest
3. **Too simple** (basic state machines) - no async handling, no resource management
4. **Poorly tested** - race conditions at 3 AM when your app shits itself in production

We needed something that gives us:
- Predictable state evolution (events in order, deterministic outcomes)
- Strong compile-time guarantees (bugs caught before they ship)
- Testable architecture (pure logic separate from side effects)
- Proper resource management (no orphaned tasks, no memory leaks)
- High performance (100K+ events/sec without breaking a sweat)

Syzygy is the result of watching too many production systems fail because of state management bugs. It's Elm Architecture for everything that isn't a web server - because most software isn't web servers.
