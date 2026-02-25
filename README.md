# Syzygy

**Elm for everything that isn't a web server** - A zero-overhead state management library for Rust targeting event-driven applications with predictable state evolution.

Perfect for desktop apps, game servers, CLI tools, IoT systems, and simple servers where events must be processed in order.

## Why Syzygy?

Most software isn't web servers - it's desktop apps, games, CLI tools, IoT devices, and simple servers. These applications share common needs:
- **Predictable state evolution** - events processed in order, deterministic outcomes  
- **Strong compile-time guarantees** - prevent bugs before they happen
- **Testable architecture** - separate pure logic from side effects
- **Resource management** - proper cleanup of files, connections, background tasks

## Features (verified)

- The Elm Architecture — Unidirectional data flow (Event → Model → Command)
- Core/Shell Separation — Pure sync Core + async Shell for effects
- Predictable Event Processing — FIFO ordering via queue + channel
- Explicit Effects — Event handlers return Commands; Shell executes effects
- TEA-first routing — parent event handlers use explicit `match` routing for child domains
- User-defined effects — No built-in effects; your types, your logic
- Executor Abstraction — Async, blocking, and resource-blocking executors
- Batch and Parallel Effect Steps — Parallelism depends on your executor
- Error-as-events — Recommended pattern; modeled in your `Event` type
- Events only need `Send` — `Rc`-backed events are welcome; `Sync` is no longer required
- Panic hooks — wire executor panics back into your event graph with `with_panic_handler`

## 5-Minute Tour

1. Import `syzygy::prelude::*` to grab the curated surface (`Command`, `Task`, `Plan`, `cmd::`, `Runner`).
2. Write a pure update function that mutates the model and returns declarative commands via `cmd::` helpers.
3. Model async work with `Task` (aka `Plan`) on your executors of choice.
4. Pick a profile (`profile_interactive`, `profile_server`, …), build the runner, queue an event, and drain until idle.

```rust
use std::time::Duration;

use syzygy::executor::{InlineAsync, Task};
use syzygy::prelude::*;

#[derive(Default)]
struct CounterModel {
    ticks: u32,
}

#[derive(Clone)]
enum CounterEvent {
    Tick,
}

#[derive(Clone)]
enum CounterEffect {
    Log(u32),
}

fn update(event: CounterEvent, model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
    match event {
        CounterEvent::Tick => {
            model.ticks += 1;
            cmd::effect(CounterEffect::Log(model.ticks))
        }
    }
}

fn effects(effect: CounterEffect, _resources: ()) -> Plan<CounterEvent, CounterEffect> {
    match effect {
        CounterEffect::Log(value) => Task::async_on::<InlineAsync, _>(async move {
            println!("tick #{value}");
            cmd::none()
        }),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
        .model(CounterModel::default())
        .event_handler(update)
        .effect_handler(effects)
        .profile_interactive()
        .with_async_executor(InlineAsync::new())
        .build();

    runner.core().try_send_event(CounterEvent::Tick)?;
    runner.drain_until_idle(Duration::from_millis(250))?;
    Ok(())
}
```

## Quick Start

```rust
use std::time::Duration;

use syzygy::executor::{Task, TokioExecutor};
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
    model: &mut AppModel,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Increment => on_increment(model),
        AppEvent::LoadData => on_load_data(model),
        AppEvent::DataLoaded { data } => on_data_loaded(model, data),
        AppEvent::Error { message } => on_error(model, message),
    }
}

fn on_increment(model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    model.counter += 1;
    cmd::effect(AppEffect::Log {
        message: format!("Counter: {}", model.counter),
    })
}

fn on_load_data(model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    model.is_loading = true;
    cmd::parallel([
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
    cmd::effect(AppEffect::Log {
        message: "Loaded data successfully".to_string(),
    })
}

fn on_error(model: &mut AppModel, message: String) -> Command<AppEvent, AppEffect> {
    model.is_loading = false;
    cmd::effect(AppEffect::Log { message })
}

#[derive(Clone)]
struct AppResources {
    log_prefix: &'static str,
}

fn effect_handler(effect: AppEffect, resources: AppResources) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::HttpRequest { url } => fetch_data(url),
        AppEffect::Log { message } => log_message(&resources, message),
    }
}

fn fetch_data(url: String) -> Task<AppEvent, AppEffect> {
    Task::async_on::<TokioExecutor, _>(async move {
        println!("Fetching: {url}");
        tokio::time::sleep(Duration::from_millis(100)).await;
        cmd::event(AppEvent::DataLoaded {
            data: "Hello from API!".to_string(),
        })
    })
}

fn log_message(resources: &AppResources, message: String) -> Task<AppEvent, AppEffect> {
    let prefix = resources.log_prefix;
    Task::async_on::<InlineAsync, _>(async move {
        println!("{prefix} {message}");
        cmd::none()
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let io_executor = TokioExecutor::builder()
        .name("app-io")
        .multi_thread()
        .worker_threads(2)
        .io()
        .build();

    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .with_resources(AppResources { log_prefix: "LOG" })
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .profile_interactive()
        // You can also skip executor registration entirely via Task::async_on::<InlineAsync, _>
        .with_async_executor(io_executor)
        .build();

    app.core().try_send_event(AppEvent::Increment)?;
    app.core().try_send_event(AppEvent::LoadData)?;

    app.run_until(|core, _| !core.model().is_loading)?;

println!("Final state: {:?}", app.core().model());
Ok(())
}
```

### Timeout pattern (retry once, celebrate success)

```rust
fn on_timeout(model: &mut AppModel, duration: Duration) -> Command<AppEvent, AppEffect> {
    model.is_loading = false;
    model.retries += 1;
    model.error = Some(format!("⏱️ took {:?}", duration));

    if model.retries < 2 {
        cmd::event(AppEvent::Retry)
    } else {
        cmd::none()
    }
}

fn retry_effect(delay_ms: u64) -> Plan<AppEvent, AppEffect> {
    Task::async_on::<TokioExecutor, _>(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        println!("✅ retry finished");
        cmd::event(AppEvent::Completed)
    })
}
```

Full example: [`examples/timeout_pattern.rs`](examples/timeout_pattern.rs).

### Panic hooks

- `with_panic_handler(|details, message| ...)` lets you surface executor panics as explicit events.

```rust
use std::sync::{Arc, Mutex};

let panic_log = Arc::new(Mutex::new(Vec::new()));
let handler_log = Arc::clone(&panic_log);

let mut runner = Syzygy::builder::<Event, Effect>()
    .with_panic_handler(move |details, message| {
        handler_log.lock().unwrap().push((details.kind, message.clone()));
        cmd::event(Event::PanicLogged(message))
    })
    .effect_handler(|_effect, _ctx| Task::none())
    .build();
```

## Queue & Backpressure Cheatsheet

| Setting | Default | When to tweak |
| ------- | ------- | ------------- |
| `with_effect_channel_capacity(None)` | Unbounded (profile overrides) | Bound it (e.g., `Some(256)`) to push back on effect storms and surface `ShellError::EffectQueueFull`. |
| `with_event_channel_capacity(None)` | Unbounded | Enable for server workloads to surface `CoreError::ChannelFull` and coordinate producers. |
| `profile_interactive()` | Idle sleep 1 ms, effect cap 256 | Great default for CLIs, desktop apps, tests. |
| `profile_server()` | Idle sleep 0 ms, caps 1024 | Favor throughput, bounded queues; pairs well with multi-executor setups. |
| `drain_until_idle(timeout)` | — | Await “all quiet” at shutdown or in tests; errors with `ShellError::Timeout` when outstanding work lingers. |

## Developer Delight

- Enable the optional `flair` feature to sprinkle emoji into startup/shutdown logs and banner output (debug builds only).
- Command helpers live in both `command::` and the shorter `cmd::` namespaces—use whichever reads best.
- `with_panic_handler` transforms executor panics into explicit events so Core stays informed.

> Tip: add `use syzygy::prelude::command;` to import lightweight helpers like `command::event(...)` and `command::parallel([...])` when you prefer DSL-style builders over associated functions.

> **Resources are cloned per effect** – use `Arc` (or other cheap-to-clone handles) for expensive dependencies like HTTP clients and DB pools. If you require interior mutability, wrap fields inside your resource struct (e.g. `Arc<Mutex<T>>`).

### Resource Ergonomics

- Keep the `Resources` type cheap to clone; prefer `Arc<_>` handles for database pools, HTTP clients, or other heavyweight fixtures.
- Initialise one-off dependencies lazily with `ResourceCell<T>` (re-exported via `syzygy::resource_cell`) and hand out clones to effects without rebuilding the underlying service.
- Only wrap the fields that need interior mutability—`Arc<Mutex<T>>` or `Arc<RwLock<T>>` scoped to a single field keeps cloning predictable and avoids coarse-grained locks.
- Effects execute according to your executor mix; leverage `SingleThreadExecutor` when mutable resources must remain single-writer.

## Perfect For

**Desktop Applications**
- GUI apps (egui, dioxus, tauri) with complex state
- Note-taking apps, IDEs, media players
- Configuration management tools

**Game Development** 
- Turn-based games with complex state machines
- Real-time games with centralized state
- Game servers and matchmaking systems

**System Tools**
- CLI tools with interactive modes
- Build systems and deployment tools
- IoT device controllers and data processors

**Simple Servers**
- Chat servers and notification systems  
- Real-time data processing pipelines
- Workflow engines and task orchestrators
- Financial systems where event ordering matters

**Not Ideal For**
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
                                                        │ (effect, resources)│
                                                        │   -> Task::async…  │
                                                        └──────────┬─────────┘
                                                                   │ spawns
                            Multiple Executors (Policy)            ▼
     ┌────────────────────────────┬────────────────────────────┬───────────────┐
     │ TokioExecutor              │ InlineAsync                │ SingleThread │
     │ (general async IO)         │ (deterministic tests)      │ (single-writer)
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
- Clones application resources per effect invocation (keep them cheap to clone)
- Your async playground - but with adult supervision

### Executors (Runtime Services)
Syzygy uses a specialized two-trait executor system that eliminates impedance mismatches between async and sync work.

#### Built-in Executors
- **`TokioExecutor`**: General async work (network, files) with `enable_all()` runtime
- **`RayonSyncExecutor`**: Parallel CPU work using Rayon's work-stealing
- **`SingleThreadExecutor`**: Sequential sync work with strict FIFO ordering
// removed executor: `ThreadPerCoreTokioExecutor` (not part of current API)

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
let io_executor = TokioExecutor::builder()
    .name("io-pool")
    .multi_thread()
    .worker_threads(4)
    .io()
    .build();

let cpu_executor = TokioExecutor::builder()
    .name("cpu-pool")
    .multi_thread()
    .worker_threads(4)
    .cpu()
    .build();

let rayon_executor = RayonExecutor::builder()
    .threads(4)
    .build();

let (core, shell) = Syzygy::builder::<MyEvent, MyEffect>()
    .model(MyModel::default())
    .event_handler(my_update)
    .effect_handler(my_effect_handler)
    .with_async_executor(io_executor)
    .with_async_executor(cpu_executor)
    .with_blocking_executor(rayon_executor)
    .build();
```

### Channel & Idle Configuration

Tune buffers and idle cadence explicitly with builder knobs:

```rust
use syzygy::prelude::Syzygy;
use syzygy::syzygy::SyzygyConfig;
use std::time::Duration;

let mut runner = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(update)
    .effect_handler(effects)
    .with_effect_channel_capacity(Some(256))
    .with_event_channel_capacity(Some(64))
    .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
    .build();
```

Suggested baselines:
- Interactive loops → idle sleep 1 ms, effect queue 256, unbounded events
- Long-running services → idle sleep 0 ms, effect & event queues 1024
- Deterministic CI runs → idle sleep 0 ms, effect & event queues 64
- Batch workloads → idle sleep 25 ms, unbounded queues

**Event backpressure**: call `.with_event_channel_capacity(Some(cap))` to bound inbound events. Senders receive `CoreError::ChannelFull` when the queue is full, letting you coordinate retries without losing determinism. Inspect the configured capacity with `core.event_channel_capacity()`.

## Performance

Syzygy is designed for high-performance event processing with deterministic behavior.

### Executor Performance
The specialized executor architecture eliminates impedance mismatches:

```rust
// IO-bound async work
Task::async_on::<TokioExecutor, _>(async move {
    let _ = reqwest::get("https://api.example.com").await;
    Command::none()
});

// CPU-bound async work on a Tokio runtime (still a future)
Task::async_on::<TokioExecutor, _>(async move {
    let _ = expensive_async_computation().await;
    Command::none()
});

// Parallel blocking work — if you register a Rayon-backed BlockingExecutor
Task::blocking_on::<RayonExecutor, _>(|| Command::none());

// Sequential blocking work with shared resource — strict FIFO
Task::blocking_with_resource_on::<SingleThreadExecutor<MyResource>, MyResource, _>(|resource| {
    resource.push("work done");
    Command::none()
});
```

## Runtime Support
Syzygy ships with production-ready executors so you can match every workload to the right engine:

- **InlineAsync** – deterministic single-threaded execution for CLIs and tests
- **TokioExecutor** – dedicated Tokio runtimes for async IO and CPU work
- **SingleThreadExecutor** – FIFO execution for blocking operations that must stay ordered
- **RayonExecutor** *(optional feature)* – parallel CPU work with Rayon

`TokioExecutor::builder()` lets you configure thread model and capabilities when creating dedicated runtimes. To reuse an existing Tokio runtime, use `TokioExecutor::from_handle(handle)` or `TokioExecutor::try_from_current()`.

Register the executor you plan to use and pair it with `Task::async_on::<YourExecutor, _>`. Inline workflows can rely on `InlineAsync::new()` for deterministic execution.

Register them directly on the builder:

```rust
let io_executor = TokioExecutor::builder()
    .name("io")
    .multi_thread()
    .worker_threads(4)
    .io()
    .build();

let cpu_executor = TokioExecutor::builder()
    .name("cpu")
    .multi_thread()
    .worker_threads(4)
    .cpu()
    .build();

Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(update)
    .effect_handler(effects)
    .with_async_executor(io_executor)
    .with_async_executor(cpu_executor)
    // optionally register blocking or resource-blocking executors here
    .build();

// Or, skip registry entirely using an explicit executor from your effect handler:
// fn effects(effect: Effect, _res: ()) -> Task<Event, Effect> {
//     match effect {
//         Effect::Fetch => Task::async_on::<InlineAsync, _>(async move {
//             // uses current Tokio runtime if present, otherwise blocks
//             Command::none()
//         }),
//     }
// }
```

Need something custom? Implement the unified `AsyncExecutor` trait, register it with `with_async_executor`, and Syzygy will drive it alongside the built-ins.

## Observability

Syzygy exposes Shell-level counters so you can wire them into metrics or tracing:

```rust
let stats = runner.shell_stats();
println!("dropped events: {}", stats.dropped_events);

let stats_handle = runner.shell_stats_handle();
metrics::counter!("syzygy_dropped_events").increment_by(stats_handle.snapshot().dropped_events as u64);
```

Counters increment whenever the shell cannot deliver events or effect steps (e.g. a consumer dropped the channel during shutdown). `Syzygy::shutdown()` now shuts down all registered executors, waits for them to finish, and emits a tracing warning if anything was dropped during teardown.

## Optional Features

- `shell` *(default)* – enable the Shell, executors, and async integration layers.
- `tca` *(optional, default off)* – enable higher-level reducer composition helpers (`scope`, `for_each`, `if_let`) for TCA-style module composition.
- `examples` and `tracing` remain optional feature flags for examples and instrumentation.

## Executor Architecture

Syzygy uses a specialized two-trait executor system for optimal performance:

### Executor Types
```rust
/// Shared lifecycle management for all executor types
pub trait ExecutorLifecycle: Send + 'static {
    fn shutdown(&self);
    fn wait(&self);
}

/// Executor specialized for async work (futures)
pub trait AsyncExecutor: ExecutorLifecycle + Sync {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError>;

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// Executor for blocking work without shared resources
pub trait BlockingExecutor: ExecutorLifecycle + Sync {
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError>;
}

/// Executor for blocking work with a dedicated mutable resource
pub trait ResourceBlockingExecutor: ExecutorLifecycle + Sync {
    fn resource_type_id(&self) -> TypeId;

    fn spawn_blocking_with_resource(
        &self,
        job: Box<dyn FnOnce(&mut dyn Any) + Send>,
    ) -> Result<(), ExecutorError>;
}
```

### Configuration
The `ExecutorRegistry<E>` maintains separate registries for async and sync executors:

```rust
pub struct ExecutorRegistry<E> {
    async_map: FxHashMap<TypeId, Arc<dyn AsyncExecutor>>,
    blocking_map: FxHashMap<TypeId, Arc<dyn BlockingExecutor>>,
    resource_blocking_map: FxHashMap<TypeId, Arc<dyn ResourceBlockingExecutor>>,
}
```

## Multi-Executor Routing

Syzygy supports routing effects to different executors based on workload type:

```rust
struct NetExec;
struct DbExec;

struct AppResources {
    http: Arc<HttpClient>,
    database: Arc<Database>,
}

fn handle_effects(effect: MyEffect, resources: AppResources) -> Task<MyEvent, MyEffect> {
    match effect {
        MyEffect::HttpGet { url } => {
            // Route to IO executor
            Task::async_on::<NetExec, _>(async move {
                let response = resources.http.get(&url).await?;
                Command::event(MyEvent::DataLoaded { data: response.body })
            })
        }
        MyEffect::SaveToDatabase { data } => {
            // Route to database executor
            Task::async_on::<DbExec, _>(async move {
                resources.database.save(&data).await?;
                Command::event(MyEvent::SaveComplete)
            })
        }
    }
}
```

## Installation

When published on crates.io:
```toml
[dependencies]
syzygy = "0.1"
tokio = { version = "1", features = ["full"] }
```

Using the repo directly (pre-release or bleeding edge):
```toml
[dependencies]
syzygy = { git = "https://github.com/ribelo/syzygy" }
tokio = { version = "1", features = ["full"] }
```

## Documentation
- [API Documentation](https://docs.rs/syzygy) (coming with the first release)
- [Examples](./examples/)
- [Architecture Guide](./docs/architecture.md)

## License
This is free and unencumbered software released into the public domain.
See UNLICENSE for details.

## Rationale

Syzygy exists because most state management solutions for Rust are either:
1. **Web-focused** (axum, warp, actix-web) - overkill for desktop apps
2. **Too complex** (full FRP systems) - learning curve steeper than Everest
3. **Too simple** (basic state machines) - no async handling, no resource management
4. **Poorly tested** - race conditions at 3 AM when your app crashes in production

We needed something that gives us:
- Predictable state evolution (events in order, deterministic outcomes)
- Strong compile-time guarantees (bugs caught before they ship)
- Testable architecture (pure logic separate from side effects)
- Proper resource management (no orphaned tasks, no memory leaks)
- High performance (100K+ events/sec without breaking a sweat)

Syzygy is the result of watching too many production systems fail because of state management bugs. It's Elm Architecture for everything that isn't a web server - because most software isn't web servers.
