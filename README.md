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

Syzygy follows **The Elm Architecture** (TEA) with clear separation between pure and impure code:

```
┌─────────────┐    Effects     ┌──────────────┐    Spawning    ┌───────────────┐
│    Shell    ├───────────────►│    Handler   ├───────────────►│   Executor    │
│ (Distributes│                │  (Processes  │                │  (Spawns &    │
│  Effects)   │                │   Effects)   │                │   Runtime)    │
└─────────────┘                └──────────────┘                └───────────────┘
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

### Executors (Runtime Services)
- Provide safe task spawning with cleanup guarantees
- Handle runtime services (timeouts, scheduling)
- Manage resources and execution contexts  
- Focus purely on spawning - no effect queue management

### Commands
- Simple data structures describing effects to run  
- No execution logic - Shell interprets them
- Can be combined with `Command::batch()` for composition
- Follow unidirectional flow: never wait for responses

#### Command Composition Patterns

**Parallel Effects** (Default):
```rust
// Effects run concurrently, events processed sequentially
Command::batch([
    Command::effect(HttpRequest { url: "api1".into() }),
    Command::effect(HttpRequest { url: "api2".into() }),
    Command::event(RefreshUI),
])
```

**Sequential Execution** (Event-Chaining):
```rust
// Use events to chain operations sequentially
match event {
    StartProcess => Command::effect(Step1),
    Step1Complete => Command::effect(Step2), 
    Step2Complete => Command::effect(Step3),
    ProcessComplete => Command::none(),
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

// Effect handler with high-performance task spawning via executor
async fn handle_effect(effect: MyEffect, ctx: EffectContext<MyEvent, MyResources, MyExecutors>) {
    match effect {
        MyEffect::ProcessBatch { items } => {
            // Get the executor from context
            let executor: &TokioExecutor<MyEvent> = ctx.executor();
            
            // Spawn multiple tasks safely - all will be cancelled on executor drop
            for item in items {
                executor.spawn(async move {
                    process_item(item).await;
                }).unwrap();
            }
        }
    }
}
```

Key performance characteristics:
- **Task spawning**: ~4ns per task (24x faster than previous implementation)
- **Memory safety**: Zero orphaned tasks through automatic cancellation on executor drop
- **Clean architecture**: Shell distributes effects, executors handle spawning

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

This project is licensed under the MIT License.
