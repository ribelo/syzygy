# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Context & Philosophy

You are working on Syzygy - a zero-overhead event-driven state management library for Rust. It provides compile-time type safety and follows The Elm Architecture (TEA) with a clean Core/Shell separation for unidirectional data flow.

**Philosophy**: Syzygy targets the **80% of applications that aren't web servers** - event-driven systems with predictable state evolution. It's "Elm for everything else" - desktop apps, game servers, CLI tools, IoT systems, and simple servers where events must be processed in order. Prioritizes compile-time safety and deterministic behavior over raw performance.

**Target Use Cases** (Event-driven applications with predictable state):
- **Desktop GUI applications** (egui, dioxus, tauri apps) - primary target
- **Game servers** - turn-based or real-time with central state management
- **IoT/embedded systems** - sensor data processing, device control  
- **CLI tools with complex state** - build systems, deployment tools, IDEs
- **Message processing systems** - chat servers, notification engines
- **Financial systems** - trading platforms, order processing (where order matters)
- **Workflow engines** - task orchestration, approval processes
- **Real-time data processing** - log aggregation, metrics collection
- **Configuration management** - infrastructure automation, deployment tools
- **Simple servers** - where state is synchronous and events need predictable ordering

**NOT ideal for**:
- **High-concurrency web servers** - where massive parallelism is core requirement
- **Distributed systems** - where state is spread across multiple nodes
- **Database systems** - where concurrent transactions are the primary feature
- **Microservices** - better suited for monolithic, single-node applications

**Current Implementation**: Simple, elegant TEA with direct function handlers, `Core/Shell` separation, and high-performance `EffectContext` for safe task spawning.

## Project Architecture Overview
- **Language**: Rust 2024 edition
- **Architecture**: Single crate library with Core/Shell separation
- **Core Design**: TEA pattern with `event_handler(event, &mut ctx) -> Command<Event, Effect>`
- **Pattern**: Error-as-events - all errors flow through same event pipeline
- **Features**: High-performance EffectContext with safety guarantees
- **Magic Handlers**: Axum-inspired parameter extraction for testable, decoupled code
- **Performance**: Handles 100K+ events/sec, optimized for GUI app scales

## Design Principles
- **Compile-time safety first**: Strong typing prevents runtime errors
- **Testable by design**: Magic handlers decouple functions from models/resources  
- **Opinionated structure**: Clear separation of sync (Core) vs async (Shell) concerns
- **Zero unnecessary allocation**: SmallVec optimizations, careful memory management
- **Error-as-events**: All failures flow through the same event pipeline

## Commands

```bash
# Build & Test
cargo build                         # Build the library
cargo build --release               # Build with optimizations
cargo test <test_name>              # Run single test (prefer this)
cargo test                          # Run all tests
cargo check --all-targets           # Quick type check
cargo clippy -- -D warnings         # Lint before commits
cargo fmt                           # Format code

# Feature Testing
cargo test --features tokio         # Test with tokio (default)
cargo test --features smol --no-default-features      # Test with smol runtime
cargo test --features async-std --no-default-features # Test with async-std runtime
cargo test --features view-model    # Test with view model support

# Benchmarking
cargo bench                         # Run all benchmarks
```

## Runtime Support

Syzygy takes a **"tokio-first with runtime flexibility"** approach:

- **Primary**: Tokio (most mature ecosystem, recommended for production)
- **Alternative**: Smol (lightweight, resource-constrained environments)
- **Alternative**: Async-std (standard library approach)
- **Custom**: Any executor through generic spawn functions

### Using Spawn Functions

```rust
use syzygy::prelude::*;

// Auto-detect runtime (recommended)
runner.run_until(condition, syzygy::spawn::spawner()).await?;

// Explicit runtime selection
runner.run_until(condition, syzygy::spawn::TokioSpawn).await?;
runner.run_until(condition, syzygy::spawn::SmolSpawn).await?;
runner.run_until(condition, syzygy::spawn::AsyncStdSpawn).await?;

// Custom spawn function
let custom_spawn = |future| my_executor.spawn(future);
runner.run_until(condition, custom_spawn).await?;
```

### Zero-Cost Async Spawning

Syzygy provides zero-cost async spawning with no boxing overhead:

```rust
// Direct zero-cost async spawning - no allocations!
syzygy::spawn::spawner().spawn(async {
    println!("This runs on the auto-detected runtime!");
});

// Runtime-specific zero-cost spawning
syzygy::spawn::TokioSpawn.spawn(async { /* tokio work */ });
syzygy::spawn::SmolSpawn.spawn(async { /* smol work */ });

// Also works with async function calls
async fn my_work() { println!("Zero-cost!"); }
syzygy::spawn::spawner().spawn(my_work());
```

## Core API Architecture

### The Syzygy Pattern - Simple TEA

```rust
use syzygy::prelude::*;

// Define events
#[derive(Debug, Clone)]
enum MyEvent {
    UserClicked,
    DataReceived { data: String },
    // Error events - no special handling needed
    ValidationError { message: String },
}

// Define effects
#[derive(Debug, Clone)]
enum MyEffect {
    HttpRequest { url: String },
    LogMessage { text: String },
}

// Define model
#[derive(Debug, Default)]
struct MyModel {
    count: i32,
    data: String,
}

// Event handler function (no trait needed!)
fn my_event_handler(event: MyEvent, ctx: &mut EventContext<MyEvent, MyEffect, Storage<MyModel, EmptyStorage>>) -> Command<MyEvent, MyEffect> {
    let model: &mut MyModel = ctx.model_mut();
    
    match event {
        MyEvent::UserClicked => {
            model.count += 1;
            Command::effect(MyEffect::LogMessage {
                text: format!("Count: {}", model.count)
            })
        }
        MyEvent::ValidationError { message } => {
            // Handle error events like any other event
            eprintln!("Error: {}", message);
            Command::none()
        }
        _ => Command::none()
    }
}

// Build the system
let (core, shell) = Syzygy::builder()
    .model(MyModel::default())
    .update(my_event_handler)
    .build();

// Set up effect handler
let shell = shell.with_effect_handler(handle_effects);

// Use Runner for orchestration
let mut runner = Runner::new(core, shell);
```

### Core API Components

1. **Event Handler**: Pure function `event_handler(event, &mut ctx) -> Command<Event, Effect>`
2. **Core**: Synchronous event processing engine that owns the model
3. **Shell**: Asynchronous effect execution with `EffectContext`
4. **Command**: Bridge between Core and Shell for effects/events  
5. **EffectContext**: High-performance, safe task spawning (24x faster)
6. **EventContext**: Safe access to models and resources in update functions
7. **Runner**: Simple orchestration of Core ↔ Shell communication
8. **Magic Handlers**: Parameter extraction for testable, decoupled functions
9. **Storage**: UnsafeCell-based model/resource chains with compile-time safety

### Error-as-Events Pattern

Core principle: **ALL errors are events**, not exceptions:

```rust
// DON'T do this - no Result returns from update()
fn bad_update(event: Event, model: &mut Model) -> Result<Command<Event, Effect>, Error> {
    // This breaks the unified pipeline
}

// DO this - errors are events
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::ProcessData { data } => {
            if data.is_empty() {
                // Error as event
                return Command::event(Event::ValidationError {
                    message: "Data cannot be empty".to_string()
                });
            }
            // Success path
            model.data = data;
            Command::effect(Effect::SaveData { data: model.data.clone() })
        }
        Event::ValidationError { message } => {
            // Handle error event like any other
            model.error_message = Some(message);
            Command::none()
        }
    }
}
```

## High-Performance EffectContext

The EffectContext provides safe, high-performance task spawning:

```rust
async fn handle_effects(effect: MyEffect, ctx: EffectContext<MyEvent, EmptyStorage>) {
    match effect {
        MyEffect::HttpRequest { url } => {
            // Spawn tasks safely - all will be cancelled on context drop
            ctx.spawn(async move {
                let response = reqwest::get(&url).await.unwrap();
                let data = response.text().await.unwrap();
                let _ = ctx.send_event(MyEvent::DataReceived { data });
            }).unwrap();
        }
        MyEffect::LogMessage { text } => {
            println!("{}", text);
        }
    }
}
```

### EffectContext Key Features:

1. **24x faster spawning** - ~4ns per task vs ~97ns in previous implementation
2. **Memory safety** - All spawned tasks automatically cancelled on context drop
3. **Zero orphaned tasks** - Hard cancellation with tokio, cooperative with other runtimes
4. **Safe concurrency** - Multiple tasks can be spawned safely

### EffectContext Safety Tests

The implementation includes comprehensive safety tests that all pass:

```bash
cargo test --test task_safety   # 5 safety tests, all passing
```

These tests verify:
- Tasks cancelled on context drop
- Multiple tasks cancelled properly
- No use-after-free issues
- Batch spawned tasks cancelled
- Cloned context safety

## Testing Patterns

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_processing() {
        let mut storage = EmptyStorage.with_model(MyModel::default());
        let event = MyEvent::UserClicked;

        let mut ctx = EventContext::new(&mut storage);
        let command = my_update(event, &mut ctx);

        let model: &MyModel = storage.get();
        assert_eq!(model.count, 1);
        // Check command contains expected effects/events
    }
}
```

### Integration Tests
Use Runner for testing complete workflows:

```rust
#[tokio::test]
async fn test_complete_flow() {
    let (core, shell) = Syzygy::builder()
        .model(MyModel::default())
        .update(my_event_handler)
        .build();

    let shell = shell.with_effect_handler(mock_effects);
    let mut runner = Runner::new(core, shell);

    // Send events and test results
    runner.core().send_event(MyEvent::UserClicked)?;
    runner.tick(syzygy::spawn::spawner()).await?;

    assert_eq!(runner.core().model().count, 1);
}
```

## Performance Characteristics

Target metrics (measured):
- Task spawning: ~4ns (24x faster than previous)
- Event processing: < 100ns
- Command creation: < 50ns
- Context creation: ~9ns

## Development Guidelines

1. **Unified Pipeline**: All events (including errors) flow through the same event_handler function
2. **Pure Event Handlers**: No side effects in `event_handler()` - only model updates and command creation
3. **Error-as-Events**: Convert all errors to events, never panic or return Results from `event_handler()`
4. **User Responsibility**: Don't panic in effects - crashes should bring down the whole app
5. **Testing First**: Write tests before implementation using magic handlers for decoupling
6. **Safety First**: All spawned tasks are tracked and cancelled automatically

## Limitations & Anti-Patterns

### ❌ **Anti-Patterns** (Don't Do This)
- **Database transactions across multiple effects**: Use effect handlers directly instead
- **Complex async coordination**: Syzygy works best when async stays on boundaries  
- **Shared mutable state outside of models**: Use resources for shared read-only data
- **Side effects in event handlers**: Keep event_handler functions pure

### ⚠️ **Limitations** (When NOT to use Syzygy)
- **High-throughput async servers**: Better suited for sync-heavy GUI applications
- **Distributed systems**: Works best when "app is your database"
- **Heavy async coordination**: Not ideal when "some async server is your database"
- **Microservices**: Better for monolithic desktop/GUI applications

### ✅ **Best Practices**
- **WebSocket connections**: Store as resources for persistence across events
- **Background jobs**: Spawn as effects with proper cleanup
- **Rate limiting/Circuit breakers**: Implement as resources with interior mutability
- **Multiple models**: Use storage chains vs single large model for better ergonomics

## Git Workflow

### Pre-Commit Checklist
```bash
cargo check --all-targets && cargo clippy -- -D warnings && cargo fmt
cargo test
```

### Commit Guidelines
- Atomic commits with clear messages
- Use conventional commit format: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`
- Never use `git add -A` (adds junk files)
- Never commit without running tests
- Each commit should represent a single logical change

## File Structure

Current implementation files:
- `src/core.rs` - Synchronous Core implementation
- `src/shell.rs` - Asynchronous Shell implementation
- `src/command.rs` - Command type and execution
- `src/async_context.rs` - High-performance EffectContext (24x faster)
- `src/builder.rs` - Simple builder pattern for system construction
- `src/runner.rs` - Application orchestration
- `src/event_context.rs` - Context for synchronous update functions
- `src/effect_handler.rs` - Effect handlers with async function traits
- `src/extract.rs` - Magic handler parameter extraction
- `src/magic_handler.rs` - Magic handler implementation
- `src/spawn.rs` - Runtime-neutral spawn functions
- `src/task.rs` - Task tracking and management
- `src/timer.rs` - Runtime-neutral timer operations
- `src/storage/` - UnsafeCell-based storage chains
- `src/error/` - Error types
- `src/lib.rs` - Public API exports

## Important Notes

- **Simple TEA**: Core implementation follows standard Elm Architecture without App trait boilerplate
- **High Performance**: EffectContext provides 24x performance improvement with safety
- **Memory Safety**: All tasks automatically cancelled to prevent leaks
- **Runtime Flexible**: Works with tokio, smol, async-std, or custom executors
- **Error-as-Events**: All errors flow through the event system
- **Magic Handlers**: Axum-inspired parameter extraction for testable, decoupled functions
- **Testing**: Comprehensive safety tests verify no orphaned tasks

## Current Status & Knowledge Gaps

✅ **Core Implementation Complete**: EffectContext, magic handlers, storage chains working
✅ **All Tests Pass**: 57 Rust source files, comprehensive test coverage including 5 critical safety tests  
✅ **Performance Verified**: 24x faster task spawning, 100K+ events/sec capability
✅ **Memory Safety**: All spawned tasks cancelled on context drop
✅ **Documentation Updated**: README and examples reflect current API
✅ **No App Trait**: Simplified API using direct function handlers

### ✅ **Architecture Questions - ANSWERED**

**Event Ordering & Determinism:**
- ✅ **FIFO Order**: `Runner::tick()` uses `VecDeque::pop_front()` - strict FIFO processing guaranteed
- ✅ **Event Replay**: Events are deterministic - same sequence produces identical state
- ✅ **Effect Ordering**: Effects are processed sequentially, no true "simultaneity" in single-threaded execution
- ✅ **No Race Conditions**: Core processes events synchronously, one at a time

**Resource Management & Lifecycle:**
- ✅ **Effect Cleanup**: In-flight effects are dropped when channels close during shutdown
- ✅ **File/Network Cleanup**: Standard Rust Drop semantics handle resource cleanup properly
- ✅ **Long-running Tasks**: Can be stored as resources or external to Syzygy with channels/JoinHandles
- ✅ **State Persistence**: User responsibility - models can implement Serialize/Deserialize as needed
- ✅ **Pause/Resume**: User responsibility - can pause by not calling `tick()` or controlling event flow

**Developer Experience & Tooling:**
- ✅ **Event Replay**: Built-in capability - events are data, can be logged/replayed  
- ✅ **State Serialization**: Depends on user structs - no forced Serialize traits
- ✅ **Debugging Tools**: Tracing integration available, `debug_logging` config in Runner
- ✅ **State Migration**: User responsibility - standard Rust enum evolution patterns apply

### 🔍 **Still Need To Validate**

**Real-World Integration (HIGH PRIORITY):**
- How do common compilation errors look? Are they helpful for beginners?
- What patterns emerge when building actual desktop applications?
- How does the async runtime integration feel in practice?

**Performance Validation (MEDIUM PRIORITY):**
- Memory usage with large state objects (100MB+ models)  
- Build time impact of magic handlers on medium-sized projects
- Actual performance with 10K+ events/sec sustained load

**Production Readiness (LOWER PRIORITY):**
- State schema evolution strategies in real applications
- Monitoring and observability patterns
- Long-running stability (memory leaks, performance degradation)
