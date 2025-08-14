# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Context

You are working on Syzygy - a zero-overhead event-driven state management library for Rust. It provides compile-time type safety using Choice types for events, tasks, and state management.

**THE Syzygy Implementation**: The primary API uses EventChoice for compile-time event dispatch with enum-like performance. All event and error handling flows through the same unified pipeline.

## Project Architecture Overview
- **Language**: Rust 2024
- **Architecture**: Single crate library with zero-overhead abstractions
- **Testing**: TDD with BDD scenarios using `craft` workflow
- **Core Design**: EventChoice/TaskChoice for compile-time dispatch, Dispatch for all handlers

### Core Components - Zero-Overhead Abstractions

- **EventChoice**: Compile-time event dispatch (no runtime HashMap lookup)
- **TaskChoice**: Compile-time task dispatch with enum performance
- **ModelChain**: Zero-overhead multi-model access with compile-time type safety
- **ResourceChain**: Zero-overhead resource access with Arc storage
- **Dispatch**: Unified result type for events and tasks (no separate error handling)

## Development Workflow

### The "Red, Green, Refactor" Cycle (Test-First Development)
1. **RED**: Write failing tests first (BDD scenarios + unit tests)
2. **GREEN**: Write minimal code to make tests pass
3. **REFACTOR**: Clean up code without changing behavior

### Spec-Driven Development Process
Before any implementation, follow this systematic approach:

1. **Plan with specs**: Use `craft spec list` to understand existing requirements and `craft spec create <feature>` for new features
2. **Follow dependencies**: Check `craft task deps` to ensure proper implementation order
3. **Write comprehensive tests**: Create BDD scenarios in `features/` directory and unit tests in `#[cfg(test)]` modules
4. **Implement real functionality**: Write actual working code, never mock implementations or stubs
5. **Validate continuously**: Run `craft spec validate` to ensure all requirements are met before marking tasks complete
6. **Commit atomically**: Make focused commits with clear messages describing the specific change

### Test Categories
- **Unit Tests**: Fast, isolated tests in `#[cfg(test)]` modules for pure functions and logic
- **Integration Tests**: Real component interactions in `tests/` directory, no mocks allowed
- **BDD Scenarios**: User-focused acceptance criteria in `features/` directory describing expected behavior
- **Benchmark Tests**: Performance tests in `benches/` directory

### Spec Organization Structure
- **Core specs**: `/specs/syzygy/` - Overall system design and architecture
- **Feature specs**: `/specs/syzygy-{feature}/` - Feature-specific functionality
- **Feature structure**: Each feature has exactly three files: `spec.md`, `design.md`, `tasks.md`

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

# Benchmarking
cargo bench --bench syzygy_benchmarks    # Run all benchmarks
./scripts/bench.sh                       # Run and save benchmark results
./scripts/bench-compare.sh [baseline]    # Compare with baseline
./scripts/bench-watch.sh                 # Watch and re-run on changes

# Craft Workflow (Spec-Driven Development)
craft status                        # Check project state
craft spec list                     # List all specifications
craft spec create <feature>         # Create new feature spec
craft task list --pending           # Find next tasks to work on
craft task start <id>               # Start working on a task
craft task complete <id>            # Mark task as completed
craft spec validate                 # Validate specs before commit
craft task deps                     # Check task dependencies

# Testing Individual Components
cargo test model::                  # Test model module
cargo test dispatch::               # Test dispatch module
cargo test resource::               # Test resource module
```

## Core API Patterns

### Event Handler Pattern (Unified Flow)
```rust
// Events are just types - including error events
#[derive(Debug, Clone)]
enum Event {
    CreateUser { email: String },
    UserCreated { email: String },
    ValidationFailed { error: String },  // Error as event
}

// Handler returns Dispatch - no special error handling
fn handle_event(event: Event, model: &mut State) -> Dispatch<Event, Command> {
    match event {
        Event::CreateUser { email } => {
            if email.is_empty() {
                // Return error as an event
                Dispatch::event(Event::ValidationFailed { 
                    error: "Empty email".to_string() 
                })
            } else {
                model.users.push(email.clone());
                Dispatch::new(
                    vec![Event::UserCreated { email }],
                    vec![Command::SaveUser { email }]
                )
            }
        }
        Event::ValidationFailed { error } => {
            // Handle error events like any other event
            model.error_count += 1;
            Dispatch::command(Command::LogError { message: error })
        }
        _ => Dispatch::none()
    }
}
```

### EventChoice Pattern (Zero-Overhead)
```rust
// Compose events at compile time
type AppEvents = EventChoice<
    CreateUser,
    EventChoice<UserCreated,
    EventChoice<ValidationFailed, NoEvent>>
>;

// Implement handler for each event type
impl HandleSyzygyEvent<AppState, AppTask> for CreateUser {
    fn handle_syzygy(&self, state: &mut AppState) -> Dispatch<Self, AppTask> {
        // Handle event
    }
}
```

### Building a Syzygy System
```rust
let (mut syzygy, handle) = Syzygy::builder()
    .model(AppState::default())
    .event_handler(handle_event)  // Simple function that returns Dispatch
    .build();

// Dispatch events
handle.dispatch(Event::CreateUser { email: "alice@example.com".to_string() })?;
syzygy.process_events();  // Zero-overhead dispatch
```

## Worker Communication & Contract Boundaries

### Worker Communication Protocol (SYZ-024)
Workers and background threads communicate back to the main event loop through the SyzygyHandle. This maintains the single-threaded event processing guarantee while allowing async work.

```rust
use tokio::spawn;
use std::time::Duration;

// Worker function that communicates via events
async fn background_worker(handle: SyzygyHandle<AppEvent>) {
    // Simulate some async work
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    match do_some_io().await {
        Ok(data) => {
            // Success - send event with data
            let _ = handle.dispatch(AppEvent::WorkCompleted { data });
        }
        Err(error) => {
            // Error - send error as event (not exception)
            let _ = handle.dispatch(AppEvent::WorkFailed { 
                error: error.to_string() 
            });
        }
    }
}

// In your main code
let handle_clone = handle.clone();
spawn(async move {
    background_worker(handle_clone).await;
});
```

### Error-to-Event Pattern (SYZ-026)
I/O errors and worker failures are converted to events rather than propagated as exceptions:

```rust
// In worker thread
async fn file_processor(handle: SyzygyHandle<FileEvent>, path: String) {
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            // Success event
            handle.dispatch(FileEvent::FileRead { 
                path, 
                contents 
            }).ok();
        }
        Err(io_error) => {
            // Error as event - no panic, no Result propagation
            handle.dispatch(FileEvent::FileError { 
                path, 
                error: io_error.to_string() 
            }).ok();
        }
    }
}
```

### Contract-Defined Boundaries (SYZ-030)
Event and Command enums define the boundaries between the functional core and imperative shell:

```rust
// Core/Shell Contract - Events (input to core)
#[derive(Debug, Clone)]
enum CoreEvent {
    // Business events
    ProcessOrder { id: u64, items: Vec<Item> },
    CancelOrder { id: u64, reason: String },
    
    // Worker completion events  
    PaymentProcessed { order_id: u64, success: bool },
    InventoryChecked { order_id: u64, available: bool },
    
    // Error events (errors as data)
    ValidationFailed { field: String, message: String },
    ExternalServiceFailed { service: String, error: String },
}

// Core/Shell Contract - Commands (output from core)
#[derive(Debug, Clone)]
enum ShellCommand {
    // Side effects to execute
    ProcessPayment { order_id: u64, amount: f64 },
    CheckInventory { order_id: u64, items: Vec<Item> },
    SendEmail { to: String, subject: String, body: String },
    
    // Logging/monitoring commands
    LogEvent { level: String, message: String },
    RecordMetric { name: String, value: f64 },
}

// Core remains pure - only transforms events to (events, commands)
fn order_handler(event: CoreEvent, model: &mut OrderState) -> Dispatch<CoreEvent, ShellCommand> {
    // Pure business logic only - no I/O, no side effects
    match event {
        CoreEvent::ProcessOrder { id, items } => {
            // Validate and store in model
            model.orders.insert(id, Order::new(items));
            
            // Return commands for shell to execute
            Dispatch::new(
                vec![CoreEvent::OrderCreated { id }],
                vec![
                    ShellCommand::ProcessPayment { order_id: id, amount: 100.0 },
                    ShellCommand::CheckInventory { order_id: id, items },
                ]
            )
        }
        // Handle error events like any other event
        CoreEvent::ValidationFailed { field, message } => {
            model.error_count += 1;
            Dispatch::command(ShellCommand::LogEvent { 
                level: "ERROR".to_string(), 
                message: format!("Validation failed: {} - {}", field, message) 
            })
        }
        _ => Dispatch::none()
    }
}
```

### Versioning Strategy
Event/Command enums can be evolved safely by:

1. **Adding new variants** (non-breaking)
2. **Adding fields to existing variants** using `#[serde(default)]` 
3. **Deprecating variants** by handling them with no-op responses
4. **Breaking changes** through major version bumps

```rust
// Version 1
#[derive(Debug, Clone)]
enum EventV1 {
    CreateUser { name: String },
}

// Version 2 - backward compatible additions
#[derive(Debug, Clone)]  
enum EventV2 {
    CreateUser { 
        name: String, 
        #[serde(default)]
        email: Option<String>  // New optional field
    },
    DeleteUser { id: u64 },    // New variant
}
```

## Development Guidelines

1. **No Backward Compatibility**: NEVER add backward compatibility aliases, deprecated functions, or legacy support. Breaking changes are acceptable and preferred over API pollution.

2. **Error-as-Events**: Errors are just events that flow through the same pipeline. No special error handling infrastructure.

3. **Zero-Overhead**: All abstractions must have zero runtime cost. Use compile-time dispatch through Choice types.

4. **Testing First**: Always write tests before implementation. Use BDD scenarios for user-facing features.

5. **Performance**: Benchmark all changes. Target <200ps for event dispatch.

6. **Clean APIs**: Prioritize clean, minimal APIs over backward compatibility.

## Git Workflow

### Pre-Commit Checklist
```bash
cargo check --all-targets && cargo clippy -- -D warnings && cargo fmt
cargo test
craft spec validate  # If using craft workflow
```

### Commit Guidelines
- Atomic commits with clear messages
- Use conventional commit format: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`
- Never use `git add -A` (adds junk files)
- Never commit without running tests
- Each commit should represent a single logical change

## Common Development Tasks

### Adding a New Event Type
1. Create spec with `craft spec create event-<name>`
2. Write BDD scenarios in `features/event-<name>.feature`
3. Add event variant to EventChoice chain
4. Implement HandleSyzygyEvent trait
5. Write unit tests
6. Update examples

### Working on Performance
1. Run baseline benchmark: `./scripts/bench.sh`
2. Make changes
3. Compare: `./scripts/bench-compare.sh baseline`
4. Target metrics:
   - Event dispatch: <200ps
   - Model access: <50ps
   - Resource access: <100ps

### Debugging Event Flow
1. Enable tracing feature: `cargo test --features tracing`
2. Set RUST_LOG=debug for detailed traces
3. Check event flow in process_events()
4. Verify EventChoice dispatch path

## Important Notes

- **Primary API**: EventChoice/TaskChoice with Dispatch - this is THE way
- **No EventOutcome**: Removed in favor of simple Dispatch
- **No ErrorChoice**: Removed - errors are just events
- **Unified Pipeline**: All events (including errors) flow through same handlers
- **Craft Workflow**: Use craft for spec-driven development when adding features