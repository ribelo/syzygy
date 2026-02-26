# Syzygy

Syzygy is a TEA-style runtime for Rust with explicit `Event -> Model -> Command` flow.

It keeps state transitions synchronous and deterministic (`Core`), while executing side effects in a separate `Shell` via declarative `Task`s.

## What Exists Today

- Pure event reducer API (`Fn(Event, &EventContext<Model>) -> Command<Event, Effect>`)
- Command routing with explicit steps:
  - `CommandStep::Event`
  - `CommandStep::Effect`
  - `CommandStep::Tracked { id, effect }`
  - `CommandStep::Cancel { id }`
- Shell-side cancellation slots (`Command::track`, `Command::cancel`)
- `Task` variants:
  - `Task::none`
  - `Task::resolved`
  - `Task::future` / `Task::once`
  - `Task::stream`
  - `Task::blocking`
- `#[derive(Model)]` extraction wrappers + runtime borrow tracking
- `TestStore` synchronous harness with exhaustive assertions, including tracked/cancel assertions

## Runtime Model

Syzygy is built around `compio`.

- When a `compio` runtime is active, `Task::future`/`Task::stream` are spawned and polled by the runtime.
- Without an active `compio` runtime, Syzygy falls back to synchronously resolving plain futures/streams in place.
- Effects that explicitly depend on `compio` APIs (for example `compio::runtime::time::sleep`) still require an active `compio` runtime.

## Quick Start

```rust
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum Event {
    Increment,
}

#[derive(Debug, Clone)]
enum Effect {
    Log,
}

#[derive(Debug, Default, Model)]
struct Model {
    counter: i32,
}

fn update(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
    match event {
        Event::Increment => {
            let counter = Counter::extract_mut(ctx);
            **counter += 1;
            Command::effect(Effect::Log)
        }
    }
}

fn effects(effect: Effect, _ctx: &EffectContext) -> Task<Event, Effect> {
    match effect {
        Effect::Log => Task::none(),
    }
}
```

## Builder API

```rust
let mut runner = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(update)
    .effect_handler(effects)
    .with_event_channel_capacity(Some(256))
    .with_syzygy_config(SyzygyConfig::default())
    .build();
```

Notes:

- `with_event_channel_capacity(Some(n))` creates a bounded channel; producers can receive `CoreError::ChannelFull`.
- Backward-compatibility methods such as `async_executor`, `compute_executor`, `blocking_executor`, and `with_effect_channel_capacity` are currently no-op shims.

## Cancellable Slots

```rust
// start/restart slot "search"
Command::track("search", Effect::Fetch)

// cancel slot "search"
Command::cancel("search")
```

`CancelId` is type-tagged. `1_u32` and `1_u64` are different slot IDs.

## TestStore Exhaustivity

`TestStore` supports consumptive assertions:

- `assert_effects(...)`
- `assert_tracked_effect(slot, effect)`
- `assert_cancelled(slot)`
- `assert_no_effects()`

In `Exhaustivity::On`, unasserted outputs (effects or cancellations) panic on next `send` or on drop.

## Development Gate

```bash
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## License

Unlicense. See `UNLICENSE`.
