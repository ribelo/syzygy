# Syzygy Architecture (First Release)

This is the map. No marketing. Just how it works and where it will bite you if you get clever.

## Core Ideas
- Unidirectional flow: Event -> update(model) -> Command -> Shell -> Task -> Event
- Pure updates, impure effects. Cross this line and you’ll hate yourself later.
- Explicit scheduling: you tell the Shell what to do; it does exactly that.

## Components

### Core (Pure, Sync)
- Owns the model (your types).
- `event_handler: fn(Event, &mut Model) -> Command<Event, Effect>`
- Processes events FIFO. Returns a `Command` (a list of steps: events/effects/batches/parallel).
- Never blocks.

### Command (Bridge)
- Thin data structure describing what to do next.
- Steps:
  - `Event(E)` – route back to Core immediately
  - `Effect(X)` – queue for the Shell
  - `Batch(Vec<X>)` – effects dispatched in order (executors decide actual overlap)
  - `Parallel(Vec<X>)` – dispatch effects without waiting between submissions; real parallelism depends on executors

### Shell (Impure, Async)
- Reads `Command` steps and executes effects via the effect handler.
- `effect_handler: fn(Effect, Resources) -> Task<Event, Effect>`
- Clones `Resources` per call; keep resources cheap to clone (use `Arc<_>` for heavy stuff).

### Task (Plan)
- Declarative plan that the Shell drives on executors.
- Variants:
  - `Event(E)` / `Events(Vec<E>)`
  - `async_on::<Exec, _>(future)` – run future on registered async executor
  - `stream_on::<Exec, _>(stream)` – forward stream items as events
  - `blocking_on::<Exec, _>(|| Command)` – blocking job (no shared resource)
  - `blocking_with_resource_on::<Exec, R, _>(|&mut R| Command)` – FIFO single-resource lane
  - `async_current(...)` / `stream_current(...)` – use current Tokio runtime if present, else block inline (no executor registration required)

### Executors (Policy)
- Register zero or more:
  - `InlineAsync` – futures run immediately on the caller thread (tests/CLIs)
  - `TokioExecutor` – dedicated tokio runtime(s)
  - `SingleThreadExecutor<R>` – blocking FIFO with mutable `R`
  - Optional: your own, by implementing the traits

## Data Flow

```
Events (external)
      |
      v
+------------+
|    Core    |
+------------+
      |
      | Command { steps }
      v
+------------+
|   Shell    |
+------------+
      |
      | effect steps  (queued)
      v
effect_handler(effect, resources)
      |
      | returns Task
      v
drive Task on executors
      |
      | route_command(Task output)
      +---------------------------+---------------------------+
      | events                    | effects / batch / parallel|
      v                           v
Core.event_rx                Shell.effect_queue
  (immediate)                   (queued work)
```

What actually happens
1. Core processes an event and spits out a `Command`.
2. `CommandStep::Event` values short-circuit straight back into Core’s channel.
3. `CommandStep::Effect/Batch/Parallel` steps land on the Shell’s effect queue.
4. Shell pops one effect, calls the effect handler, and gets a `Task`.
5. The Task either pushes events directly or resolves to another `Command`.
6. `route_command` splits the Task output using the same rules as step 2/3.
7. Rinse until everyone stops screaming.

## Error Handling
- No special channels. Effects emit error events; you model them in `Event`.
- Missing executor -> `ShellError::TaskSpawnFailed` when scheduling the `Task`.

## Backpressure & Queues
- Core event channel: unbounded (default). `Core::pending_count()` for snapshots.
- Shell effect channel: configurable (bounded/unbounded). `Shell::pending_effects()` for snapshots.

## Patterns That Scale
- Chain complex logic via `Command::events([..])` rather than mutating all at once.
- Use `Batch` when you need deterministic submission order; use `Parallel` when the work items are independent and your executors can overlap them.
- Use `SingleThreadExecutor<R>` when a resource explodes under concurrency.
- Use `async_current` to avoid wiring executors when you already run under `#[tokio::main]`.

## Footguns (documented)
- Doing I/O in the event handler. Don’t. Return a `Command::effect` instead.
- Returning a `Task::async_on::<Exec, _>` without registering `Exec`.
- Cloning heavy `Resources` values per effect call. Use handles (`Arc<_>`).

## Minimal Example
```rust
use syzygy::prelude::*;
use syzygy::executor::{Task, InlineAsync};

#[derive(Clone)] enum E { Inc, Saved }
#[derive(Clone)] enum X { Save }
#[derive(Default)] struct M { n: i32 }

fn update(e: E, m: &mut M) -> Command<E, X> {
    match e {
        E::Inc => { m.n += 1; Command::effect(X::Save) }
        E::Saved => Command::none(),
    }
}

fn effects(x: X, _r: ()) -> Task<E, X> {
    match x {
        X::Save => Task::async_on::<InlineAsync, _>(async move {
            // pretend to save; then notify core
            Command::event(E::Saved)
        })
    }
}
```
