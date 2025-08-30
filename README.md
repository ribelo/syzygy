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
use syzygy::prelude::*;
use syzygy::event_context::EventContext;
use std::collections::HashMap;

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
fn my_event_handler(event: AppEvent, ctx: &mut EventContext<AppEvent, AppEffect, Storage<AppModel, EmptyStorage>>) -> Command<AppEvent, AppEffect> {
    let model: &mut AppModel = ctx.model_mut();
    
    match event {
        AppEvent::Increment => {
            model.counter += 1;
            Command::effect(AppEffect::Log { 
                message: format!("Counter: {}", model.counter) 
            })
        }
        AppEvent::LoadData => {
            model.is_loading = true;
            Command::effect(AppEffect::HttpRequest { 
                url: "https://api.example.com/data".to_string() 
            })
        }
        AppEvent::DataLoaded { data } => {
            model.data = Some(data);
            model.is_loading = false;
            Command::none()
        }
        AppEvent::Error { message } => {
            eprintln!("Error: {}", message);
            model.is_loading = false;
            Command::none()
        }
    }
}

// Effect handler (converts effects to async operations)
async fn handle_effects(
    effect: AppEffect,
    ctx: syzygy::async_context::EffectContext<AppEvent, EmptyStorage>,
) {
    match effect {
        AppEffect::HttpRequest { url } => {
            println!("🌐 Fetching: {}", url);
            
            // For now, just do the async work directly without spawning
            // TODO: This example will be updated when executor integration is complete
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            
            let _ = ctx.send_event(AppEvent::DataLoaded { 
                data: "Hello from API!".to_string() 
            });
        }
        AppEffect::Log { message } => {
            println!("📝 {}", message);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build the system using Storage-based API
    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .update(my_event_handler)
        .build();
    
    // Set up the effect handler
    let shell = shell.with_effect_handler(handle_effects);
    
    // Use Runner for automatic orchestration
    let mut runner = Runner::new(core, shell);
    
    // Send some events
    runner.core().send_event(AppEvent::Increment)?;
    runner.core().send_event(AppEvent::LoadData)?;
    
    // Run until data loads
    runner.run_until(
        |core, _shell| !core.model().is_loading,
        syzygy::spawn::spawner() // Auto-detect runtime
    ).await?;
    
    println!("Final state: {:?}", runner.core().model());
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

Event → Core.update() → Command → Shell.execute() → Effect Handler → Executor → Event
```

### Core (Pure)
- Manages application state synchronously
- Processes events through `event_handler()` function
- Returns Commands describing what effects to run
- No I/O, no async, no side effects

### Shell (Impure)  
- Catches effects from Core commands
- Distributes effects to appropriate handlers
- Routes events back to Core for processing
- Orchestrates the async execution flow
- **Sequential by default**: Effects run one after another for predictable behavior
- **Parallel coordination**: Uses `futures_concurrency` for join/race patterns in effect handlers

### Executors (Runtime Services)
- Provide safe task spawning with cleanup guarantees
- Handle runtime services (timeouts, scheduling)
- Manage resources and execution contexts  
- Each executor offers different execution guarantees

#### Built-in Executors

Syzygy provides multiple executor types for different use cases:

- **`TokioExecutor`** — General-purpose async I/O executor using tokio runtime
  - Best for: HTTP requests, file I/O, database queries
  - Execution: Concurrent task spawning

- **`SingleThreadExecutor`** — Strict FIFO sequential execution on dedicated OS thread
  - Best for: Database writes, file operations requiring strict ordering
  - Execution: Sequential FIFO guarantee, no race conditions

- **`ThreadPerCoreTokioExecutor`** — Actix-like model with one tokio runtime per CPU core
  - Best for: High-throughput applications, avoiding runtime contention
  - Execution: Round-robin distribution across isolated per-core runtimes

- **`RayonExecutor`** (feature `rayon`) — CPU-bound work-stealing compute pool
  - Best for: Pure computation, image processing, mathematical operations  
  - Execution: Work-stealing across CPU threads, no async I/O

### Commands
- Simple data structures describing effects to run  
- No execution logic - Shell interprets them
- Can be combined with `Command::batch()` for composition
- Follow unidirectional flow: never wait for responses

#### Command Composition Patterns

**Sequential Effects** (Default):
```rust
// Effects run sequentially, predictable execution order
Command::batch([
    Command::effect(HttpRequest { url: "api1".into() }),
    Command::effect(HttpRequest { url: "api2".into() }),  // Waits for api1
    Command::event(RefreshUI),  // Processed after both effects
])
```

**Parallel Coordination with futures_concurrency**:
```rust
// Use Command::parallel for effects that should run concurrently
Command::parallel([
    HttpRequest { url: "api1".into() },
    HttpRequest { url: "api2".into() },  // Runs concurrently with api1
])

// In effect handlers, use futures_concurrency for coordination
use futures_concurrency::prelude::*;

async fn handle_effects(effect: AppEffect, ctx: EffectContext<...>) {
    match effect {
        AppEffect::FetchMultipleApis { urls } => {
            // Join: wait for all to complete
            let futures = urls.into_iter().map(|url| async move {
                reqwest::get(&url).await?.text().await
            });
            
            match futures.collect::<Vec<_>>().join().await {
                Ok(responses) => {
                    let _ = ctx.send_event(AppEvent::AllApisCompleted { responses });
                }
                Err(error) => {
                    let _ = ctx.send_event(AppEvent::ApiError { error: error.to_string() });
                }
            }
        }
        
        AppEffect::RaceToFirstResponse { urls } => {
            // Race: return first successful response
            let futures = urls.into_iter().map(|url| async move {
                reqwest::get(&url).await?.text().await
            });
            
            match futures.collect::<Vec<_>>().race().await {
                Ok(first_response) => {
                    let _ = ctx.send_event(AppEvent::FirstApiResponded { response: first_response });
                }
                Err(error) => {
                    let _ = ctx.send_event(AppEvent::AllApisFailed { error: error.to_string() });
                }
            }
        }
    }
}
```

**Executor Routing Strategies**:
```rust
async fn handle_effects(effect: AppEffect, ctx: EffectContext<...>) {
    match effect {
        AppEffect::DatabaseWrite { data } => {
            // Use sequential executor for consistent writes
            ctx.executor::<SingleThreadExecutor<_>, _>()
                .spawn(async move { write_to_db(data).await });
        }
        AppEffect::ImageProcess { image } => {
            // Use compute executor for CPU-heavy work
            ctx.executor::<RayonExecutor<_>, _>()
                .spawn(async move { process_image(image).await });
        }
        AppEffect::ParallelRequests { urls } => {
            // Use general executor with futures_concurrency
            ctx.executor::<TokioExecutor<_>, _>().spawn(async move {
                let responses = urls.into_iter()
                    .map(|url| reqwest::get(&url))
                    .collect::<Vec<_>>()
                    .join()  // All requests in parallel
                    .await;
                // Process responses...
            });
        }
    }
}
```

## Key Concepts

### Events
Everything that happens in your app (user clicks, data loads, errors) is an event:

```rust
#[derive(Debug, Clone)]
enum MyEvent {
    UserClicked,
    DataLoaded { result: String },
    NetworkError { reason: String },
}
```

### Model  
Your application state - pure data, no behavior:

```rust
#[derive(Debug)]
struct MyModel {
    users: Vec<User>,
    is_loading: bool,
    error_message: Option<String>,
}
```

### Effects
Descriptions of side effects you want to perform:

```rust
#[derive(Debug, Clone)]
enum MyEffect {
    HttpGet { url: String },
    SaveToDatabase { data: String },
    ShowNotification { message: String },
}
```

### Error-as-Events
All errors become events - no exceptions, no Result returns from update():

```rust
fn update(&self, event: MyEvent, model: &mut MyModel) -> Command<MyEvent, MyEffect> {
    match event {
        MyEvent::LoadUser { id } => {
            if id.is_empty() {
                // Error becomes an event
                Command::event(MyEvent::ValidationError { 
                    field: "id".to_string(),
                    message: "ID cannot be empty".to_string()
                })
            } else {
                Command::effect(MyEffect::LoadUser { id })
            }
        }
        MyEvent::ValidationError { field, message } => {
            // Handle error like any other event
            model.error_message = Some(format!("{}: {}", field, message));
            Command::none()
        }
    }
}
```

## Setup Options

### Auto-Wired (Default)
The default `build()` automatically connects Shell to Core:

```rust
let (core, shell) = Syzygy::builder::<MyApp>()
    .app(MyApp)
    .model(MyModel::default())
    .build();  // Shell automatically connected to Core

let shell = shell.with_effect_handler(handle_effect);
```

### Manual Wiring (Advanced Use Cases)
Use `build_manual()` for manual control over connections:

```rust
let (core, shell) = Syzygy::builder::<MyApp>()
    .app(MyApp)
    .model(MyModel::default())
    .build_manual();

// Manually connect Shell to Core
let event_sender = core.event_sender();
let shell = shell
    .with_effect_handler(handle_effect)
    .with_event_sender(event_sender);
```

## Performance

Syzygy achieves high performance through:

- **24x faster task spawning** - Zero-cost EffectContext with hard task cancellation
- **Zero-overhead commands** - Commands compile to simple data structures
- **Memory safety guarantees** - All spawned tasks cancelled on context drop
- **Efficient composition** - Commands can be combined with minimal overhead

### Executor Performance

Executors provide safe, high-performance task spawning with cleanup guarantees:

```rust
use syzygy::prelude::*;

// Effect handler with per-executor routing
async fn handle_effect(effect: MyEffect, ctx: EffectContext<MyEvent, MyResources, MyExecutors>) {
    match effect {
        MyEffect::ProcessBatch { items } => {
            // Route CPU-intensive work to dedicated executor
            let executor: &ThreadPerCoreTokioExecutor<MyEvent> = ctx.executor();
            executor.spawn(async move {
                for item in items { 
                    process_item(item).await; 
                }
                let _ = ctx.send_event(MyEvent::BatchComplete);
            }).unwrap();
        }
        MyEffect::DatabaseWrite { data } => {
            // Route sequential operations to single-thread executor
            let executor: &SingleThreadExecutor<MyEvent> = ctx.executor();
            executor.spawn(async move {
                write_to_database(data).await;
                let _ = ctx.send_event(MyEvent::DatabaseUpdated);
            }).unwrap();
        }
    }
}
```

Key performance characteristics:
- **Task spawning**: ~4ns per task (24x faster than previous implementation)
- **Memory safety**: Zero orphaned tasks through automatic cancellation on executor drop
- **Sequential by default**: Effects execute predictably without race conditions
- **Parallel coordination**: `futures_concurrency` provides efficient join/race patterns
- **Executor specialization**: Choose the right executor for your workload's needs
- **Zero-allocation futures**: Structured concurrency without unnecessary heap allocations

## Runtime Support

Syzygy takes a **"tokio-first with runtime flexibility"** approach to async runtime support.

### Runtime Priority

- **🥇 Tokio** (Primary) - Most mature ecosystem, recommended for production
- **🥈 Smol** - Lightweight alternative for resource-constrained environments  
- **🥉 Async-std** - Standard library approach, good for educational purposes

When multiple runtime features are enabled, Syzygy automatically uses the highest priority runtime.

### Quick Runtime Selection

```rust
use syzygy::prelude::*;

// Auto-detect runtime (recommended)
runner.run_until(condition, syzygy::spawn::spawner()).await?;

// Or be explicit about your runtime choice
runner.run_until(condition, syzygy::spawn::TokioSpawn).await?;
```

### Zero-Cost Async Spawning (Rust 1.85+)

Syzygy provides zero-cost async spawning with no boxing overhead:

```rust
use syzygy::spawn::{spawner, Spawn, TokioSpawn};

// Direct zero-cost async spawning via unified spawner
spawner().spawn(async {
    println!("This async block runs on the auto-detected runtime!");
});

// Runtime-specific zero-cost spawning
TokioSpawn.spawn(async {
    println!("This runs specifically on tokio with zero overhead!");
});
```

**Performance**: Unlike traditional spawn APIs that require boxing (`Box<dyn Future>`), 
these functions accept futures directly, eliminating all allocation overhead!

### Why Tokio-First?

While Syzygy's core is runtime-neutral through generic `spawn_fn` parameters, we acknowledge practical reality:

- **Ecosystem**: Tokio has the largest ecosystem of compatible crates
- **Production**: Most production Rust applications use tokio
- **Documentation**: Most examples and tutorials assume tokio
- **Maintenance**: Testing and optimization primarily focus on tokio

### Runtime Neutrality Details

Under the hood, Syzygy achieves runtime neutrality through:

- **Spawn trait**: `Runner` and `Shell` accept any `impl Spawn`
- **Timer abstractions**: Runtime-agnostic sleep and timeout operations
- **Feature flags**: Clean separation between runtime-specific code

This means you can:
- Use custom executors or thread pools
- Switch runtimes without changing application logic
- Run tests with different runtimes for compatibility verification

```rust
// Custom spawn function example
// Provide your own spawner by implementing `Spawn`
struct MySpawner;
impl syzygy::spawn::Spawn for MySpawner {
    fn spawn(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        my_thread_pool.spawn(future);
    }
}

runner.run_until(condition, MySpawner).await?;
```

## Multi-Executor Routing

Route a single `Effect` enum to different executors using magic extraction. `EffectContext::run_on` builds a per-executor context with that executor’s resources and awaits completion (Batch stays strictly ordered; ParallelEffects fans out and each branch awaits on its chosen executor).

```rust
use syzygy::prelude::*;

#[derive(Clone)] struct HttpClient;
#[derive(Clone)] struct Database;

struct NetExec(ThreadPerCoreTokioExecutor<AppEvent>);
struct DbExec(SingleThreadExecutor<AppEvent>);

async fn handle_effects(effect: AppEffect, ctx: EffectContext<AppEvent, AppResources, (DbExec, NetExec)>) {
    match effect {
        AppEffect::HttpRequest { url } => {
            ctx.run_on::<NetExec, _>(url, |url: String, client: &HttpClient, tx: EventSender<AppEvent>| async move {
                // ... do http, send event
            }).await.unwrap();
        }
        AppEffect::DatabaseWrite { op } => {
            ctx.run_on::<DbExec, _>(op, |op: WriteData, db: &Database, tx: EventSender<AppEvent>| async move {
                // ... single-writer op, send event
            }).await.unwrap();
        }
    }
}
```

## Installation

### Tokio (Recommended)

```toml
[dependencies]
syzygy = { git = "https://github.com/ribelo/syzygy" }
# Default feature includes tokio
tokio = { version = "1", features = ["full"] }
```

### Alternative Runtimes

```toml
[dependencies]
# For smol runtime
syzygy = { git = "https://github.com/ribelo/syzygy", default-features = false, features = ["smol"] }
smol = "2.0"

# For async-std runtime  
syzygy = { git = "https://github.com/ribelo/syzygy", default-features = false, features = ["async-std"] }
async-std = { version = "1.13", features = ["attributes"] }

# Optional: Enable tracing for debugging
# syzygy = { git = "https://github.com/ribelo/syzygy", features = ["tracing"] }
```

## Documentation

- [docs/architecture.md](docs/architecture.md) - Core design principles and patterns
- [docs/command-composition-analysis.md](docs/command-composition-analysis.md) - Command composition patterns and analysis
- [docs/timeout-handling.md](docs/timeout-handling.md) - Timeout handling patterns
- [docs/runner-event-cycle.md](docs/runner-event-cycle.md) - Runner orchestration details
- [docs/unidirectional-architecture.md](docs/unidirectional-architecture.md) - Architecture decision rationale
- [CLAUDE.md](CLAUDE.md) - Development guide for Claude Code
- [Examples](examples/) - Usage examples and demonstrations

## License

## Rationale

Why events?
- Predictability: Unidirectional flow yields deterministic state evolution.
- Testability: `update(event, model)` is pure; easy to unit test and reason about.
- Composability: Everything (including errors) is just another event.
- Decoupling: No ad-hoc request/response backchannels; all state changes cross the same gate.

Why effects (as data)?
- Separation of concerns: Core describes “what to do”; Shell/handlers decide “how to do it”.
- Observability: Effects can be logged, inspected, and replayed without executing them.
- Runtime neutrality: Handlers can target different executors without changing Core logic.
- Safety: No hidden side effects inside update; easier to reason about failure and retries.

Why multiple executors?
- Correctness policy: Different work needs different execution guarantees.
  - SingleThreadExecutor: Enforce single-writer semantics (e.g., DB writes) with strict FIFO.
  - ThreadPerCoreTokioExecutor: High-throughput async IO with minimal cross-core contention.
  - TokioExecutor: General async runtime integration; dedicate a runtime just for effects.
  - RayonExecutor (feature): CPU-heavy compute on a work‑stealing pool, isolated from IO loops.
- Isolation: Effects never contend with the event loop; executors own their runtimes/threads.
- Performance: Pick the optimal engine per effect without splitting the effect type.

How routing works (simple mental model)
- Core emits `Command` with effects.
- Shell enforces composition (Batch = sequential, ParallelEffects = concurrent).
- Effect handler matches the effect and calls `ctx.run_on::<Executor>(payload, |..| async { .. })`.
- The handler closure runs where it belongs (chosen executor), injects needed resources, and sends events back.

This project is licensed under the MIT License.
