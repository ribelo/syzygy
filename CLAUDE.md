# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Context

You are working on Syzygy - a zero-overhead event-driven state management library for Rust. It provides compile-time type safety and follows functional core/imperative shell architecture with a unified event/command dispatch system.

**THE Syzygy Implementation**: The primary API uses a simple single model with event handlers that return `Dispatch<Event, Command>` for zero-overhead event processing.

## Project Architecture Overview
- **Language**: Rust 2024 edition
- **Architecture**: Single crate library with zero-overhead abstractions
- **Core Design**: Single model with event handlers returning Dispatch objects
- **Pattern**: Error-as-events - all errors flow through same event pipeline
- **Features**: Optional async command execution, deterministic testing, benchmarking

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
cargo test --features async         # Test with async features
cargo test --features tracing       # Test with tracing enabled
cargo test --features parallel      # Test with parallel features

# Benchmarking
cargo bench --bench dispatch_benchmark    # Run dispatch benchmarks
cargo bench                              # Run all benchmarks

# BDD Test Execution (custom harness)
cargo test --test builder                # Builder feature tests
cargo test --test event_processing       # Event processing tests
cargo test --test state_management       # State management tests
cargo test --test error_handling         # Error handling tests
cargo test --test type_safety           # Type safety tests
cargo test --test system_properties     # System properties tests
cargo test --test architecture          # Architecture tests
```

## Core API Architecture

### THE Syzygy Pattern - Simple and Clean
The current implementation uses a straightforward approach:

```rust
use syzygy::prelude::*;

// Define your state
#[derive(Debug, Default)]
struct AppState {
    users: Vec<String>,
    count: i32,
}

// Define events - including error events
#[derive(Debug, Clone)]
enum AppEvent {
    CreateUser { name: String },
    UserCreated { name: String },
    // Error events - no special handling needed
    ValidationError { field: String, message: String },
    UserCreationFailed { reason: String },
}

// Define commands/tasks for side effects
#[derive(Debug, Clone)]
enum AppTask {
    LogUserCreated { name: String },
    SaveToDatabase { data: String },
}

// Event handler - pure function returning Dispatch
fn handle_events(event: AppEvent, model: &mut AppState) -> Dispatch<AppEvent, AppTask> {
    match event {
        AppEvent::CreateUser { name } => {
            if name.trim().is_empty() {
                // Return error as event - no exceptions
                return Dispatch::event(AppEvent::ValidationError {
                    field: "name".to_string(),
                    message: "Name cannot be empty".to_string()
                });
            }
            
            model.users.push(name.clone());
            Dispatch::new(
                vec![AppEvent::UserCreated { name: name.clone() }],
                vec![AppTask::LogUserCreated { name }]
            )
        }
        AppEvent::ValidationError { field, message } => {
            // Handle error events like any other event
            println!("Validation error in {}: {}", field, message);
            Dispatch::none()
        }
        _ => Dispatch::none()
    }
}

// Build the system
let (mut syzygy, handle, _executor) = Syzygy::builder()
    .model(AppState::default())
    .event_handler(handle_events)
    .build();

// Dispatch events
handle.dispatch(AppEvent::CreateUser { name: "Alice".to_string() })?;
syzygy.process_events();
```

### Builder API Components

1. **SyzygyBuilder**: Main builder for creating the system
2. **Model**: Single application state (not chains)
3. **Resources**: Optional shared state for command execution
4. **Event Handler**: Pure function transforming events to dispatch
5. **Command Handler**: Optional async function for side effects

### Error-as-Events Pattern

Core principle: **ALL errors are events**, not exceptions:

```rust
// DON'T do this - no Result returns from handlers
fn bad_handler(event: Event, model: &mut State) -> Result<Dispatch<Event, Command>, Error> {
    // This breaks the unified pipeline
}

// DO this - errors are events
fn good_handler(event: Event, model: &mut State) -> Dispatch<Event, Command> {
    match event {
        Event::CreateUser { name } => {
            if name.is_empty() {
                // Error as event
                return Dispatch::event(Event::ValidationFailed { 
                    field: "name".to_string(),
                    reason: "Empty name".to_string()
                });
            }
            // Success path
            Dispatch::event(Event::UserCreated { name })
        }
        Event::ValidationFailed { field, reason } => {
            // Handle error event like any other
            model.error_count += 1;
            Dispatch::command(Command::LogError { field, reason })
        }
    }
}
```

## Async Command Execution

When async side effects are needed:

```rust
use std::future::Future;
use std::pin::Pin;

// Async command handler
fn handle_commands(
    ctx: CommandContext<AppEvent, Resources>,
    command: AppTask
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async move {
        match command {
            AppTask::SaveToDatabase { data } => {
                // Async I/O operation
                match save_to_db(&data).await {
                    Ok(_) => {
                        // Success - send event back
                        let _ = ctx.handle().dispatch(AppEvent::DataSaved { data });
                    }
                    Err(error) => {
                        // Error as event - no panic
                        let _ = ctx.handle().dispatch(AppEvent::SaveFailed { 
                            error: error.to_string() 
                        });
                    }
                }
            }
            AppTask::LogUserCreated { name } => {
                println!("User created: {}", name);
            }
        }
    })
}

// Build with command handler
let (mut syzygy, handle, executor) = Syzygy::builder()
    .model(AppState::default())
    .event_handler(handle_events)
    .command_handler(handle_commands)
    .build();
```

## Testing Patterns

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let mut model = AppState::default();
        let event = AppEvent::CreateUser { name: "Alice".to_string() };
        
        let result = handle_events(event, &mut model);
        
        assert_eq!(model.users.len(), 1);
        assert!(matches!(result, Dispatch { events, .. } if !events.is_empty()));
    }
}
```

### BDD Feature Tests
Feature files in `features/` directory use custom harness for BDD-style testing.

### Deterministic Testing
```rust
use syzygy::prelude::*;

// Use EventRecorder for deterministic testing
let recorder = EventRecorder::new();
let scenario = TestScenario::builder()
    .events(vec![
        AppEvent::CreateUser { name: "Alice".to_string() },
        AppEvent::CreateUser { name: "Bob".to_string() },
    ])
    .build();

let result = TestUtils::run_scenario(scenario);
assert!(result.is_success());
```

## Performance Characteristics

Target metrics (from benchmarks):
- Model access: ~7ns
- Model updates: ~7ns  
- Resource access: ~15ns
- Event dispatch: ~51ns
- Async task spawn: ~900ns

## Development Guidelines

1. **Unified Pipeline**: All events (including errors) flow through the same handlers
2. **Pure Event Handlers**: No side effects in event handlers - only model updates and dispatch
3. **Error-as-Events**: Convert all errors to events, never panic or return Results
4. **Testing First**: Write tests before implementation
5. **Zero-Overhead**: All abstractions must compile to optimal code
6. **Clean APIs**: Prioritize simple, understandable APIs

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

- `src/syzygy.rs` - THE main Syzygy implementation
- `src/builder.rs` - SyzygyBuilder for system construction
- `src/dispatch.rs` - Dispatch type for event/command results
- `src/handle.rs` - SyzygyHandle for event dispatch
- `src/context.rs` - CommandContext for async handlers
- `src/model.rs` - Model trait and implementations
- `src/resource.rs` - Resources for shared state
- `examples/` - Working examples showing usage patterns
- `features/` - BDD feature files with custom test harness
- `benches/` - Performance benchmarks

## Important Notes

- **Primary API**: Single model with event handlers returning Dispatch
- **No Magic**: Simple, straightforward API without complex abstractions
- **Error-as-Events**: All errors flow through the event system
- **Optional Features**: Async execution, tracing, parallel processing
- **Performance Focus**: Zero-overhead abstractions with benchmarking