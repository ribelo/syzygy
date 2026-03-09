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
3. **Explicit effects.** All finite I/O goes through `Effect -> Task`, and all long-lived external sources go through `Subscription`. Handlers are pure.
4. **FIFO event ordering.** Events processed in arrival order. No prioritization.
5. **Bounded resource growth.** 256 fields per model. Bounded event channel (configurable).
6. **Explicit extraction surface.** `#[derive(Model)]` does not expose field extractors unless the field opts in with `#[model(...)]`.

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

0. On the first `step()`, an optional boot command runs once before ordinary channel events
1. `Core::try_send(Event)` pushes to channel
2. `step()` drains channel, calls `handler(event, ctx)`
3. Handler returns `Command` (events + effects)
4. Events immediately re-enqueued to channel
5. Effects passed to Shell

First-step order is:
- boot command
- core event processing
- subscription reconciliation
- shell drain

Steady-state order after boot is:
- core event processing
- subscription reconciliation
- shell drain

### Effect Path (Asynchronous)

1. Shell receives `CommandStep::Effect(X)`
2. Looks up handler, calls `handler(effect, ctx)`
3. Returns `Task`:
   - `Task::None`: nothing happens
   - `Task::Resolved(cmd)`: command executed synchronously
   - `Task::Future(fut)`: spawned on configured runtime backend
   - `Task::Stream(s)`: spawned, each item processed
   - `Task::process(spec, map_result)`: shell-owned subprocess, killed on task cancellation
   - `Task::process_interactive(spec, on_update)`: shell-owned subprocess with streamed updates and lease-addressed stdin control
   - `Task::Blocking(f)`: shell-owned blocking work, shutdown waits for completion
   - `Task::BlockingCooperative(f)`: blocking work with cooperative cancellation
4. Task completion produces `Command`, loops back to Core

Shell progression is synchronous. There is no async `drain`/`step` API; async is confined to effect execution on Syzygy's owned runtime.

### Runtime Clock

- `syzygy::runtime::sleep(...)` binds to the clock owned by the active `Runtime` when the future is polled.
- Default runtimes use wall time.
- `Runtime::manual()` returns `(Runtime, ManualClock)` for deterministic tests.
- Built-in timers like `Subscription::every(...)` and shell internal grace/backpressure waits use the same clock path.
- Manual time is advanced explicitly by tests; it is not an app capability.

### Subscription Path (State-Derived)

1. `Syzygy::step()` recomputes `Subscription` from the current model through a read-only `SubscriptionContext`
2. Shell diffs desired subscriptions against currently active subscriptions by explicit key
3. New subscriptions start, unchanged subscriptions keep running, removed subscriptions are stopped
4. Driver updates map back into `Command` values and re-enter normal shell routing

**Important:** event/effect handlers never receive sender channels or runtime handles. Any channel or callback machinery needed by a subscription driver stays inside the driver implementation.

## Module Dependencies

```
lib.rs
├── core.rs           # Event processing, no async
├── shell.rs          # Command routing, task management
├── command.rs        # Command/step types
├── extract.rs        # EventContext, borrow tracking
├── executor/
│   └── task.rs       # Task variants
├── process.rs        # ProcessSpec and subprocess execution
├── subscription.rs   # Pure Subscription descriptions and driver registry
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

## Explicit Field Extraction

```rust
#[derive(Model)]
struct AppModel {
    #[model(wrapper = Counter)]
    counter: i32,
    #[model(part)]
    settings: Settings,
}
```

**Rules:**
- `#[derive(Model)]` alone only enables whole-model extraction.
- `#[model(wrapper = Name)]` generates an explicit wrapper extractor with that exact name.
- `#[model(part)]` extracts the field type directly and requires the type to be unique within the model.
- Unannotated fields remain ordinary state and are invisible to field-level extraction.

## Abortable Task Ownership

```rust
let lease = TaskLease::new();
Command::abortable(lease.clone(), effect) // Start/replace lease-owned task
Command::cancel(lease)                    // Explicit stop
```

For the common one-owned-field-in-model case:

```rust
#[derive(Model)]
struct AppModel {
    #[model(wrapper = SaveJob)]
    save_job: AbortSlot,
}

fn start_save(save_job: &mut SaveJob) -> Command<Event, Effect> {
    save_job.start(Effect::Save)
}
```

**Semantics:**
- A lease owns at most one abortable task. New `abortable` with the same lease drops the old task.
- `AbortSlot::start()` replaces the stored lease and emits an explicit cancel for the previous task before starting the next one.
- The shell also cancels abortable work when the last owner of the lease disappears.
- `Task::process` and `Task::process_interactive` are shell-owned too: dropping the task kills the child process, so explicit cancel, owner loss, replacement, and shutdown all terminate subprocesses.
- Interactive process stdin is controlled through lease-addressed commands (`process_write` / `process_close_stdin`), not raw child handles.
- Default process cancellation is `CloseStdinThenKill { grace: 500ms }`; if stdin is not piped, the shell skips the grace wait and kills immediately.
- Stdout/stderr streaming is FIFO per channel. No cross-channel total-order guarantee.
- Lease-owned blocking work must be cooperative. `Task::blocking` is rejected for abortable effects; use `Task::blocking_cooperative` and check the `BlockingCancelToken`.
- Mapping abortable child commands/tasks is explicit. Plain `map` rejects abortable steps; `TaskLeaseScope` remaps leases when a caller intentionally embeds child abortable work into a parent domain.

**ABA Protection:** Generation token per active lease entry. Prevents "cancel wrong task" race.

## Subscriptions

```rust
fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
    handle!(describe_subscriptions, ctx)
}
```

**Semantics:**
- `Subscription` is state-derived and read-only. It describes which long-lived sources should exist now.
- Each subscription has an explicit key. Shell diffs by key plus driver/spec equality.
- Same key + same driver/spec keeps the running source alive and swaps in the newest mapper closure.
- Same key + changed driver/spec cancels the old source and starts a new one.
- Missing key cancels the old source.
- Shutdown cancels every active subscription.
- `Shell` stays model-typed, so a shell carrying a subscription handler cannot be recombined with a different `Core<Model>`.
- Custom `SubscriptionDriver::subscribe(...)` construction runs inside Syzygy's owned runtime task. Synchronous reconciliation only validates that the driver is registered.
- Custom drivers that want deterministic time should use `syzygy::runtime::sleep(...)` instead of backend-specific timer APIs.
- Built-in `Subscription::every` is always available. App-specific integrations use `Subscription::custom::<Driver, _, _>(...)` plus `builder.with_subscription_driver(driver)`.

## Boot

```rust
let app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(handle_event)
    .boot_handler(|_model| Command::event(Event::Boot))
    .build()?;
```

**Semantics:**
- Boot is a one-shot startup hook, not a long-lived lifecycle.
- It is evaluated against the current model and returns an ordinary `Command`.
- Boot-generated events are inserted ahead of already queued external events for the first step.
- Splitting and recombining `(Core, Shell<Model>)` preserves pending boot work because boot lives in the model-typed shell.

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

**Shutdown:** Drop `Syzygy`. Pending async/process tasks are cancelled, subprocesses are killed, and non-abortable blocking work is awaited to completion.

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
| `examples/` | 12 progressive tutorials |

## When It Breaks

**"already borrowed mutably" panic:** Handler tried to borrow same field twice, or mixed `&Model` with `&mut Field`. Fix: Remove duplicate borrow or use scoped extraction.

**"exceeds borrow tracker capacity (256)":** Model has >256 fields. Fix: Split into nested models with `#[model(part)]`.

**"Resource not found":** Effect handler requested resource not registered. Fix: Add `.with_resource()` during build.

**Channel full:** Bounded channel overflow. Fix: Increase capacity or add backpressure.
