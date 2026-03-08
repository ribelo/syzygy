# Syzygy Architecture

System shape and invariants. For 3am incident response.

## Scope

**In scope:** Synchronous state machine with explicit async effect boundaries.

**Non-goals:**
- General actor framework (use actix)
- Distributed consensus
- Hot code reloading
- Persistent state management (user implements)

## Invariants (Non-Negotiable)

1. **Single-threaded Core.** All state transitions happen on one thread. No `Send` bound on `Model`.
2. **No aliasing.** `&mut Model` and `&Field` to overlapping memory is impossible. Runtime panic on violation.
3. **Explicit effects.** All I/O must go through `Effect -> Task`. Handlers are pure.
4. **FIFO event ordering.** Events processed in arrival order. No prioritization.
5. **Bounded resource growth.** 256 fields per model. Bounded event channel (configurable).

## Component Diagram

```
┌─────────────┐     Event      ┌─────────────┐
│   External  │───────────────>│    Core     │
│   Sources   │                │             │
└─────────────┘                │  ┌───────┐  │
                               │  │ Model │  │
                               │  └───────┘  │
                               └──────┬──────┘
                                      │ Command
                                      ▼
                               ┌─────────────┐
                               │    Shell    │
                               │             │
                               │ ┌─────────┐ │
                               │ │  Queue  │ │
                               │ └─────────┘ │
                               └──────┬──────┘
                                      │ Task
                                      ▼
                               ┌─────────────┐
                               │   Runtime   │
                               │ (pluggable) │
                               └─────────────┘
```

## Data Flow

### Event Path (Synchronous)

1. `Core::try_send_event(Event)` pushes to channel
2. `step()` drains channel, calls `handler(event, ctx)`
3. Handler returns `Command` (events + effects)
4. Events immediately re-enqueued to channel
5. Effects passed to Shell

### Effect Path (Asynchronous)

1. Shell receives `CommandStep::Effect(X)`
2. Looks up handler, calls `handler(effect, ctx)`
3. Returns `Task`:
   - `Task::None`: nothing happens
   - `Task::Resolved(cmd)`: command executed synchronously
   - `Task::Future(fut)`: spawned on configured runtime backend
   - `Task::Stream(s)`: spawned, each item processed
4. Task completion produces `Command`, loops back to Core

Shell progression is synchronous. There is no async `drain`/`step` API; async is confined to effect execution.

## Module Dependencies

```
lib.rs
├── core.rs           # Event processing, no async
├── shell.rs          # Command routing, task management
├── command.rs        # Command/step types
├── extract.rs        # EventContext, borrow tracking
├── executor/
│   └── task.rs       # Task variants
└── builder.rs        # Syzygy::builder()
```

**Dependency rule:** `core` does not depend on `shell`. `shell` depends on `core`.

## Borrow Tracking

```rust
EventContext {
    ptr: *mut Model,
    whole_model_mut: Cell<bool>,
    whole_model_immut: Cell<u32>,
    fields_mut: Cell<[u64; 4]>,      // 256 bits
    fields_immut: Cell<[u64; 4]>,   // 256 bits
}
```

**Algorithm:**
- Field index `i` maps to bit `i % 64` in word `i / 64`
- Set bit on borrow, check overlap, panic if collision
- `BorrowGuard` snapshots state, restores on drop

**Complexity:** O(1) per borrow. No heap allocation.

## Abortable Task Ownership

```rust
let lease = TaskLease::new();
Command::abortable(lease.clone(), effect) // Start/replace lease-owned task
Command::cancel(lease)                    // Explicit stop
```

**Semantics:**
- A lease owns at most one abortable task. New `abortable` with the same lease drops the old task.
- The shell also cancels abortable work when the last owner of the lease disappears.
- Lease identity is unique, so command/task mapping does not need cancellation-specific namespacing.

**ABA Protection:** Generation token per slot entry. Prevents "cancel wrong task" race.

## Error Handling

| Layer | Error Type | Response |
|-------|------------|----------|
| Event handler | Panic | Crash (deliberate: logic bug) |
| Effect handler | Panic | Crash (deliberate: setup bug) |
| Channel send | `CoreError::ChannelFull` | Return to caller (bounded) or block (unbounded) |
| Async task | `Command::event(ErrorEvent)` | User-defined recovery |

**No automatic retry.** Failed effects produce events; user decides.

## Performance

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Event dispatch | O(1) | Direct call, no allocation |
| Field borrow | O(1) | Bitmask check |
| Command routing | O(n) steps | Iterates command steps |
| Task spawn | O(1) | configured runtime backend |
| Cancellation | O(1) | Slot map lookup |

**Memory:**
- `EventContext`: 40 bytes (stack)
- `Command`: SmallVec (no alloc for ≤4 steps)
- `Shell` queue: Reused VecDeque (amortized zero alloc)

## Operational Notes

**Logs:** None internal. User adds tracing in handlers.

**Metrics:** None internal. User increments counters in handlers.

**State location:** User owns `Model`. Syzygy holds during `step()`.

**Shutdown:** Drop `Syzygy`. Pending tasks cancelled. No graceful drain.

## Testing Strategy

| Component | Tool | Pattern |
|-----------|------|---------|
| Pure handlers | `TestStore` | Send events, assert state |
| Effects | `TestStore::receive_async` | Mock effect responses |
| Integration | `Syzygy::builder()` | Full stack with mock resources |

**Determinism:** `TestStore` executes synchronously. Async effects mocked.

## File Locations

| File | Purpose |
|------|---------|
| `src/core.rs` | Event processing |
| `src/shell.rs` | Command routing, task lifecycle |
| `src/extract.rs` | Borrow tracking, `EventContext` |
| `src/command.rs` | `Command`, `CommandStep` |
| `src/executor/task.rs` | `Task` variants |
| `examples/` | 10 progressive tutorials |

## When It Breaks

**"already borrowed mutably" panic:** Handler tried to borrow same field twice, or mixed `&Model` with `&mut Field`. Fix: Remove duplicate borrow or use scoped extraction.

**"exceeds borrow tracker capacity (256)":** Model has >256 fields. Fix: Split into nested models with `#[model(part)]`.

**"Resource not found":** Effect handler requested resource not registered. Fix: Add `.with_resource()` during build.

**Channel full:** Bounded channel overflow. Fix: Increase capacity or add backpressure.
