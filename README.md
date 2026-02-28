# Syzygy

Zero-overhead TEA (The Elm Architecture) for Rust. Pure synchronous state transitions. Explicit async effects.

```rust
use syzygy::prelude::*;

#[derive(Model)]
struct Model { counter: i32 }

#[derive(Clone)]
enum Event { Increment }

fn inc(counter: &mut Counter) {
    **counter += 1;
}

fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, ()> {
    match event {
        Event::Increment => handle!(inc, ctx),
    }
}
```

## Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
syzygy = { git = "https://github.com/ribelo/syzygy" }
```

Run an example:

```bash
cargo run --example 01_basic_counter
```

## Core Concepts

| Concept | Purpose | Example |
|---------|---------|---------|
| `Model` | Application state | `#[derive(Model)] struct App { count: i32 }` |
| `Event` | Something that happened | `enum Event { Increment }` |
| `Effect` | Side-effect to execute | `enum Effect { Save }` |
| `Command` | Instructions from handler | `Command::event(E)` or `Command::effect(X)` |
| `Task` | Async execution unit | `Task::future(async { ... })` |
| `handle!` | Call handler with context | `handle!(increment, ctx, amount)` |
| `emit` | Send event to Core | `core.emit(Event::Tick)` |

## The `handle!` Macro

The `handle!` macro connects event data to handler functions:

```rust
fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
    match event {
        // No payload: handle!(handler, ctx)
        Event::Tick => handle!(tick, ctx),
        
        // Single payload: handle!(handler, ctx, payload)
        Event::Increment(n) => handle!(add, ctx, n),
        
        // Multiple payloads: handle!(handler, ctx, arg1, arg2, ...)
        Event::CreateUser(name, age) => handle!(create_user, ctx, name, age),
    }
}
```

Handler signatures use extracted field types:

```rust
// Wrapper type - double deref to access inner value
fn add(n: i32, counter: &mut Counter) -> Command<Event, Effect> {
    **counter += n;
    Command::none()
}

// #[model(part)] - direct access
fn update(config: &mut Config) -> Command<Event, Effect> {
    config.version += 1;
    Command::none()
}
```

## The TEA Loop

```
Event -> Handler(&mut Model) -> Command -> Shell -> Task -> Event
```

1. **Core** receives `Event`, calls handler with mutable model access
2. Handler returns `Command` (events, effects, or both)
3. **Shell** routes commands: events loop back to Core, effects spawn Tasks
4. **Task** executes async work, produces new Events

## Field Extraction

`#[derive(Model)]` generates transparent wrapper types for borrow tracking:

```rust
#[derive(Model)]
struct Model {
    counter: i32,
    #[model(part)]  // Use type directly, no wrapper
    settings: Settings,
}

// Wrapper type: double deref
fn inc(counter: &mut Counter) { **counter += 1; }

// Extracted type: direct access
fn update(settings: &mut Settings) { settings.theme = Dark; }
```

| Approach | When to Use | Handler Signature |
|----------|-------------|-------------------|
| Wrapper (default) | Multiple fields of same type | `&mut Counter` |
| `#[model(part)]` | Unique complex types | `&mut Settings` |
| Whole model | Needs all fields | `&mut Model` |

`#[model(direct)]` is accepted as an alias for `#[model(part)]`.

**Constraint:** 256 fields max per model. Runtime panic on aliasing violations (`&mut T` + `&T` overlap).

## Effects and Tasks

```rust
enum Effect { Fetch(String) }

async fn fetch(url: String) -> Command<Event, Effect> {
    let data = reqwest::get(&url).await.text().await.unwrap();
    Command::event(Event::Fetched(data))
}
```

| Task Type | Use For | Returns |
|-----------|---------|---------|
| `Task::none()` | No side effect | Nothing |
| `Task::once(async)` | One-shot async | Single event |
| `Task::stream(impl Stream)` | Ongoing streams | Multiple events |
| `Task::blocking(fn)` | CPU-intensive work | Single event |

## Command Composition

```rust
Command::effect(Effect::Save)
    .and_event(Event::Saved)           // Add event
    .map_event(|e| Event::Child(e))    // Transform
    .track("save", Effect::Backup)     // Cancellable slot
```

## Testing

```rust
use syzygy::prelude::*;

let mut store = TestStore::new(Model::default(), handle_event);
store.emit(Event::Increment);

assert_eq!(store.state().counter, 1);
store.assert_effects([Effect::Save]);
```

**Note:** `emit()` is fire-and-forget (returns `()`). Use `try_send()` if you need to handle channel errors explicitly.

## Footguns

**Aliasing panics at runtime.** This will panic:

```rust
fn bad(ctx: &EventContext<Model>) {
    let _ = Model::extract(ctx);        // &Model
    let _ = Counter::extract_mut(ctx);  // &mut i32 inside Model
}
```

**Resource cloning on every effect access.** Expensive resources should be wrapped in `Arc<T>`:

```rust
#[derive(Clone)]
struct DbPool(Arc<Pool>);  // Cheap clone

// NOT: struct DbPool(Pool)  // Expensive clone every effect
```

**256 field limit.** Exceeding this panics at model construction.

**CancelId type-sensitivity.** `1u32` and `1u64` are different slots.

## Development

Quality gate (run before commit):

```bash
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Run specific example:

```bash
cargo run --example 10_todo_app
```

## Examples

| Example | Concepts |
|---------|----------|
| `01_basic_counter` | Wrapper types, pure handlers |
| `02_field_extraction` | `#[model(part)]` vs wrappers |
| `03_child_models` | Nested models, dispatch delegation |
| `04_async_effects` | `Task::future`, loading states |
| `05_stream_effects` | `Task::stream`, tickers |
| `06_resources` | Dependency injection |
| `07_testing` | `TestStore` patterns |
| `08_command_composition` | `Command::and`, `map_event` |
| `09_error_handling` | Result/Option in handlers |
| `10_todo_app` | Full application |

## License

Unlicense. See `UNLICENSE`.
