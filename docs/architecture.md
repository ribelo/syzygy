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
2. **Runtime-enforced aliasing safety.** Extractor overlap (`&mut` + overlapping `&`/`&mut`) is checked at runtime and panics as a programmer error on violation.
3. **Explicit effects.** All finite I/O goes through `Effect -> Task`, and all long-lived external sources go through `Subscription`. Handlers are pure.
4. **FIFO event ordering.** Events processed in arrival order. No prioritization.
5. **Bounded extraction tracking.** `#[derive(Model)]` tracks at most 256 opt-in extracted fields (`#[model(...)]`) and rejects larger sets during macro expansion. Event channel capacity is configurable.
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

### Shell Snapshot

- `Shell::snapshot()` returns lightweight runtime diagnostics without requiring trace recording.
- Snapshot includes active lease-owned tasks, active subscription keys, deferred/untracked/queue/error counts, and in-flight activity count.
- Snapshot also carries `last_termination` with explicit target (`Task`, `Process`, `Subscription`) and reason (`Completed`, `Failed`, `Cancelled(...)`).
- Cancellation reasons are explicit (`ExplicitCommand`, `Replacement`, `OwnerDropped`, `SubscriptionReconciled`, `Shutdown`) so teardown intent is visible during live debugging.
- Snapshot is observational only; it does not expose runtime control handles.

### Structured Trace Recorder

- Tracing is runtime infrastructure, not an app capability: enable it through `DiagnosticsConfig.trace(ShellTraceConfig { ... })`.
- `Shell::trace_snapshot()` returns ordered `ShellTraceEntry` values with monotonic sequence numbers.
- Trace events cover:
  - core/shell phases (`BootCommandDispatched`, `CoreEventsProcessed`, `SubscriptionsReconciled`, `ShellDrained`)
  - command routing (`CommandStep::Event/Effect/Abortable/Cancel/ProcessWrite/ProcessCloseStdin`)
  - task/subscription lifecycle (`TaskSpawned`, `SubscriptionStarted`, `SubscriptionUpdated`)
  - process stdin control and termination (`ProcessControl`, `Termination`)
- Default is disabled to keep baseline overhead minimal; when enabled, storage is bounded by `max_entries` (ring-buffer eviction from oldest).

### Subscription Path (State-Derived)

1. `Syzygy::step()` recomputes `Subscription` from the current model through a read-only `SubscriptionContext`
2. Shell diffs desired subscriptions against currently active subscriptions by explicit key
3. New subscriptions start, unchanged subscriptions keep running, removed subscriptions are stopped
4. Driver updates map back into `Command` values and re-enter normal shell routing

**Important:** event/effect handlers never receive sender channels or runtime handles. Any channel or callback machinery needed by a subscription driver stays inside the driver implementation.

## Runtime Work Decision Guide

Pick the primitive by lifecycle shape, not by implementation convenience:

| Scenario | Preferred primitive | Why | Avoid |
|----------|---------------------|-----|-------|
| Event triggers one finite async operation | `Command::effect` + `Task::once` (or `Task::future`) | Work has a clear start/end and returns a bounded command | Using `Subscription` for one-shot requests |
| Event triggers finite stream-like work with explicit owner | `Command::effect` + `Task::stream` (typically lease-owned) | Stream lifetime should follow app-owned task ownership | Global subscription for workflow-local stream |
| Need shell-owned subprocess result | `Task::process` | Shell owns child lifecycle and cancellation on drop/shutdown | Spawning child manually inside generic async/blocking task |
| Need interactive subprocess control | `Task::process_interactive` + lease-addressed process commands | Keeps stdin writes/cancel semantics explicit and model-owned | Exposing child handles to model/event handlers |
| Need long-lived external source while state predicate is true | `Subscription::{every, custom}` | Source lifecycle is state-derived and reconciled by key | Re-emitting recurring effects from event handlers |
| Need replace/cancel semantics tied to model state | `AbortSlot` / `TaskLease` | Ownership is explicit; replacement and owner-drop cancellation are deterministic | Starting abortable work without retaining owner |

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
- Common driver adapters are explicit and local to the driver impl: `SubscriptionDriver::poll`, `poll_with`, and `stream` reduce repetitive glue while keeping specs as pure data and registration explicit.
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

| Test goal | Tool | Pattern | Avoid |
|-----------|------|---------|-------|
| Verify pure event logic and emitted effects | `TestStore` | Send events, assert state/effects synchronously | `RunnerTester` for simple reducer tests |
| Verify effect-to-event mapping only | `TestStore::receive_async` | Mock effect completions directly | Full runtime when shell behavior is irrelevant |
| Verify subscriptions, process tasks, shell errors, or manual time | `RunnerTester` | Real runner + bounded `drain()` + `advance_time(...)` | Mocking these paths in `TestStore` |
| Verify production wiring (drivers/resources/config) | Integration with `Syzygy::builder()` | Build full app and drive real commands/events | Assuming unit tests cover wiring failures |

**Determinism:** `TestStore` executes synchronously with mocked effects. `RunnerTester` uses a real `Syzygy` runner plus `Runtime::manual()` so subscriptions and runtime-owned tasks can advance through explicit `advance_time(...)`.

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

**"already borrowed mutably" panic:** Handler tried to borrow same field twice, or mixed `&Model` with `&mut Field`. This is a deliberate programmer-error panic; messages include overlap guidance and caller location. Fix: Remove duplicate borrow or use scoped extraction.

**"exceeds borrow tracker capacity (256)":** Model has >256 `#[model(...)]` fields. This is rejected during macro expansion. Fix: Split extraction into nested models with `#[model(part)]` or reduce extracted fields.

**Effect handler does not compile:** A signature-injected resource is missing from the builder environment. Fix: add `.with_resource(...)` before `.effect_handler(...)`, or wrap same-shaped resources in distinct newtypes.

**"Effect resource ... is not registered":** Dynamic/manual resource extraction requested a resource that was not registered. This is a deliberate programmer-error panic with caller location and `.with_resource(...)` hint. Fix: Add `.with_resource()` during build or prefer signature injection through `handle!(...)` for compile-time checking.

**Channel full:** Bounded channel overflow. Fix: Increase capacity or add backpressure.
