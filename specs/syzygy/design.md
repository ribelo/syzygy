# Syzygy Design

## Architecture Overview

Syzygy is a zero-overhead event-driven state management library for Rust that enforces clean separation between functional core and imperative shell through a unified Event → Dispatch<Event, Command> pattern.

## Core Architecture Decisions

### 1. Model-Based State Management (SYZ-002)

**Decision**: Use a single model to represent application state, passed as `&mut Model` to event handlers.

**Rationale**: 
- Simplifies state management compared to multiple models
- Provides clear ownership and mutation control
- Enables immediate consistency within event processing
- Matches functional programming principles with controlled mutation

**Implementation**: The `SyzygyBuilder` requires a model via `.model(state)` and handlers receive `fn(Event, &mut Model) -> Dispatch<Event, Command>`.

### 2. Unified Dispatch Pattern (SYZ-006, SYZ-007)

**Decision**: Event handlers return `Dispatch<Event, Command>` containing both new events and side effects as data.

**Rationale**:
- Pure functional core - handlers have no side effects
- Testable and deterministic - same input always produces same output
- Clear separation between business logic (events) and side effects (commands)
- Stack-allocated ArrayVec for zero heap allocations in common cases

**Implementation**: 
```rust
pub struct Dispatch<Event, Command> {
    pub events: ArrayVec<Event, 8>,    // New events to process
    pub commands: ArrayVec<Command, 8>, // Side effects to execute
}
```

### 3. Core/Shell Separation (SYZ-023)

**Decision**: Strict separation between functional core (event processing) and imperative shell (command execution).

**Rationale**:
- Event handlers must be pure - no I/O, no side effects
- Commands represent side effects as data structures
- Enables deterministic testing and replay
- Supports clean architecture principles

**Implementation**:
- Event handlers: `fn(Event, &mut Model) -> Dispatch<Event, Command>` (pure)
- Command handlers: `fn(CommandContext, Command) -> Future<()>` (imperative)

### 4. Single-Threaded Event Processing (SYZ-027)

**Decision**: All event processing happens in a single thread with sequential, deterministic processing.

**Rationale**:
- Eliminates race conditions and concurrent access issues
- Simplifies reasoning about state changes
- Enables deterministic behavior for testing
- Maintains consistency without locks

**Implementation**: The main `process_events()` loop processes events sequentially from a channel.

### 5. Channel-Based Architecture (SYZ-028)

**Decision**: Use channels for event communication with a `SyzygyHandle` for external interaction.

**Rationale**:
- Decouples event sources from the core system
- Enables non-blocking event dispatch
- Supports worker communication through events
- Provides clear interface boundaries

**Implementation**:
```rust
let (syzygy, handle) = Syzygy::builder()
    .model(state)
    .event_handler(handler)
    .build();

handle.dispatch(event)?;  // Non-blocking
syzygy.process_events();  // Processes all queued events
```

### 6. Panic Recovery (SYZ-014)

**Decision**: Isolate handler panics to prevent system crashes while continuing operation.

**Rationale**:
- User code errors shouldn't crash the entire system
- Logging provides visibility into issues
- System remains operational for other events
- Graceful degradation under error conditions

**Implementation**: `catch_unwind()` around handler execution, returning `Dispatch::none()` on panic.

### 7. Resource Management (SYZ-009)

**Decision**: Resources container provides immutable access during event processing, mutable access during command execution.

**Rationale**:
- Event handlers must remain pure (immutable access only)
- Commands need to perform side effects (mutable access allowed)
- Type-safe resource injection through generic container
- Shared state managed through Arc for thread safety

**Implementation**:
```rust
// Event handler: immutable access
fn handle_event(event: Event, model: &mut Model) -> Dispatch<Event, Command>

// Command handler: mutable access via CommandContext
fn handle_command(ctx: CommandContext<Event, Resources>, cmd: Command) -> Future<()>
```

### 8. Worker Communication Protocol (SYZ-024, SYZ-026)

**Decision**: Workers communicate back through events, not direct state mutation.

**Rationale**:
- Maintains single-threaded event processing guarantee
- Errors become events rather than exceptions
- Preserves deterministic behavior
- Workers act as external clients

**Implementation**: Workers receive `SyzygyHandle` and send completion/error events.

### 9. Stack-Allocated Performance (Performance Goal)

**Decision**: Use `ArrayVec` with stack allocation for typical event/command counts.

**Rationale**:
- Zero heap allocations for up to 8 events/commands
- Predictable performance characteristics
- Optimal for the common case while supporting edge cases
- Maintains zero-overhead abstraction principle

**Implementation**: `ArrayVec<T, 8>` in `Dispatch` with fallback to heap if needed.

### 10. Compile-Time Type Safety (SYZ-016, SYZ-017)

**Decision**: Strong typing for events and commands with `Send + Clone` bounds.

**Rationale**:
- Prevents runtime type errors
- Enables safe cross-thread communication
- Clear contracts at compile time
- Performance through static dispatch

**Implementation**: Generic types `Syzygy<Model, Event, Command, Resources>` with trait bounds.

## API Design Patterns

### Builder Pattern
```rust
let (syzygy, handle) = Syzygy::builder()
    .model(AppState::default())
    .resource(database)
    .resource(config)
    .event_handler(handle_event)
    .command_handler(handle_command)
    .build();
```

### Event Handler Pattern
```rust
fn handle_event(event: Event, model: &mut AppState) -> Dispatch<Event, Command> {
    match event {
        Event::UserSignup { email } => {
            model.users.push(User::new(email.clone()));
            Dispatch::new(
                vec![Event::UserCreated { email }],
                vec![Command::SendWelcomeEmail { email }]
            )
        }
        Event::Error { message } => {
            model.error_count += 1;
            Dispatch::command(Command::LogError { message })
        }
    }
}
```

### Command Handler Pattern
```rust
async fn handle_command(ctx: CommandContext<Event, Resources>, cmd: Command) {
    match cmd {
        Command::SendWelcomeEmail { email } => {
            match send_email(&email).await {
                Ok(_) => ctx.dispatch(Event::EmailSent { email }).ok(),
                Err(e) => ctx.dispatch(Event::Error { 
                    message: format!("Email failed: {}", e) 
                }).ok(),
            }
        }
    }
}
```

## Performance Characteristics

- **Event Dispatch**: <200ps overhead per event
- **Model Access**: Direct reference, zero overhead
- **Resource Access**: Arc deref overhead only
- **Memory**: Stack allocation for typical workloads
- **Deterministic**: Same events → same state transitions

## Testing Strategy

The architecture enables comprehensive testing:

1. **Unit Tests**: Pure event handlers are easily testable
2. **Integration Tests**: Full event flow validation
3. **Property Tests**: Deterministic replay of event sequences
4. **Performance Tests**: Benchmarking with predictable behavior

This design provides the foundation for a fast, reliable, and maintainable event-driven system.
