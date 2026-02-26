# Syzygy Architecture

This document describes the current implementation, not planned APIs.

## Flow

```
Event -> Core(update) -> Command steps -> Shell -> Task execution -> Event
```

- `Core` owns model state and runs event handlers synchronously.
- Event handlers return `Command<Event, Effect>`.
- `Shell` interprets command steps and executes effects.

## Core

`Core` responsibilities:

- Hold model
- Drain inbound event channel
- Process events FIFO
- Return produced commands

Channel behavior:

- `with_event_channel_capacity(None)` uses an unbounded channel.
- `with_event_channel_capacity(Some(n))` uses a bounded channel.
- Sending to a full bounded channel returns `CoreError::ChannelFull`.

## Command Steps

- `Event(E)`: routed back to Core
- `Effect(X)`: execute effect handler
- `Tracked { id, effect }`: run effect in cancellation slot; previous slot task is cancelled/replaced
- `Cancel { id }`: cancel currently tracked task in slot

`CancelId` is type-aware (`TypeId + hash`). Different Rust types with same value are different IDs.

## Shell

`Shell` routes command steps and tracks asynchronous activity.

Key guarantees:

- Activity counter is guard-based (decrements on completion or cancellation)
- Tracked slot cleanup uses generation tokens to avoid ABA deletion races
- Spawned command routing is iterative (queue-based), preventing recursive stack growth
- Tracked task cancellation drops only the current slot entry

## Task Execution

`Task` variants:

- `None`
- `Resolved(Command)`
- `Future(Future<Output = Command>)`
- `Stream(Stream<Item = Command>)`

Execution strategy:

- With active `compio` runtime: futures/streams are spawned and polled asynchronously.
- Without active runtime: plain futures/streams are resolved synchronously as fallback.
- Effects that directly use `compio` runtime facilities still require active runtime context.

## Runner (`Syzygy`)

`step()`:

1. Core processes queued events
2. Shell dispatches resulting commands
3. Shell drains runtime once

`run()` and `run_until(...)`:

- Loop over `step()`
- If idle work is pending, park through `compio` runtime polling when available
- Fall back to thread sleep/yield only when no runtime is active

## EventContext Safety

`EventContext` uses runtime borrow tracking for mutable extraction:

- field-bit tracking for duplicate mutable extraction
- byte-range overlap tracking for aliasing violations
- borrow state guard restores tracking state per handler call, enabling safe sequential composition

This preserves soundness of unsafe extraction paths while allowing composed reducer calls.

## TestStore

`TestStore` mirrors Core behavior in-process and buffers outputs for assertions.

Tracked/cancel semantics are modeled explicitly:

- tracked effects are slot-aware and replace prior slot effect
- cancellations remove pending tracked effect for that slot
- exhaustive mode requires asserting both effects and cancellations

Assertions are consumptive (drain-on-assert), enabling deterministic, explicit test traces.
