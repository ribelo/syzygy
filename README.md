# Syzygy

TEA (The Elm Architecture) for Rust with explicit runtime costs and boundaries. Pure synchronous state transitions. Explicit async effects.

```rust
use syzygy::prelude::*;

#[derive(Model)]
struct Model {
    #[model(wrapper = Counter)]
    counter: i32,
}

#[derive(Clone)]
enum Event { Increment }

fn inc(counter: &mut Counter) {
    *counter.get_mut() += 1;
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

## Runtime Backends

Syzygy shell execution is runtime-agnostic via Cargo features:

- Default: `rt-compio`
- Optional: `rt-tokio`

```toml
# default (compio)
syzygy = { git = "https://github.com/ribelo/syzygy" }

# tokio backend
syzygy = { git = "https://github.com/ribelo/syzygy", default-features = false, features = ["shell", "rt-tokio"] }
```

For deterministic time-based tests, build a runtime with a manual clock and inject it:

```rust
let (runtime, clock) = syzygy::runtime::Runtime::manual()?;

let mut app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .with_runtime(runtime)
    .event_handler(handle_event)
    .effect_handler(handle_effect)
    .build()?;

clock.advance(Duration::from_secs(60));
app.step()?;
```

`syzygy::runtime::sleep(...)` now binds to Syzygy's owned clock when the future is polled. Built-in timers like `Subscription::every(...)` and custom drivers that use `syzygy::runtime::sleep(...)` participate in manual time automatically. Manual clocks are intended for explicit `advance(...)` plus `step()` control, not for wall-clock waiting.

## Core Concepts

| Concept | Purpose | Example |
|---------|---------|---------|
| `Model` | Application state | `#[derive(Model)] struct App { #[model(wrapper = Count)] count: i32 }` |
| `Event` | Something that happened | `enum Event { Increment }` |
| `Effect` | Side-effect to execute | `enum Effect { Save }` |
| `Command` | Instructions from handler | `Command::event(E)` or `Command::effect(X)` |
| `Task` | Effect execution unit | `Task::future(async { ... })` |
| `Subscription` | State-owned external event source | `Subscription::every(Key::Clock, Duration::from_secs(1), Event::Tick)` |
| `boot_handler` | One-shot startup command | `.boot_handler(|model| Command::event(Event::Boot))` |
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
// Wrapper type - explicit helper methods
fn add(n: i32, counter: &mut Counter) -> Command<Event, Effect> {
    *counter.get_mut() += n;
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

## Boot

Use `boot_handler(...)` for one-shot startup work instead of fake startup events pushed from outside.

```rust
fn boot(_model: &Model) -> Command<Event, Effect> {
    Command::event(Event::LoadInitialState)
}

let app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(handle_event)
    .boot_handler(boot)
    .build()?;
```

Boot semantics are explicit:

- runs exactly once
- receives `&Model` and returns a normal `Command<Event, Effect>`
- boot-generated events are enqueued before prequeued external channel events
- first-step order is: boot command, core event processing, subscription reconciliation, shell drain
- steady-state order after boot is: core event processing, subscription reconciliation, shell drain

## Field Extraction

`#[derive(Model)]` is inert until a field opts into extraction:

```rust
#[derive(Model)]
struct Model {
    #[model(wrapper = Counter)]
    counter: i32,
    #[model(part)]  // Use the field type directly
    settings: Settings,
}

// Wrapper type: explicit access helpers
fn inc(counter: &mut Counter) { *counter.get_mut() += 1; }

// Extracted type: direct access
fn update(settings: &mut Settings) { settings.theme = Dark; }
```

Generated wrapper helpers keep explicit extraction readable:

```rust
fn mark_loaded(counter: &mut Counter) {
    *counter.get_mut() += 1;
}

fn ensure_name(name: &mut MaybeName) {
    let value = name.get_or_insert("guest".to_string());
    value.push('!');
}
```

| Approach | Attribute | When to Use | Handler Signature |
|----------|-----------|-------------|-------------------|
| Named wrapper | `#[model(wrapper = Counter)]` | Repeated/simple field types | `&mut Counter` |
| Direct part | `#[model(part)]` | Unique complex field types | `&mut Settings` |
| Whole model | none | Needs all fields | `&mut Model` |

**Constraint:** 256 opt-in extracted fields max per model. `#[derive(Model)]` rejects larger extracted sets during macro expansion. Aliasing violations (`&mut T` + `&T` overlap) are enforced at runtime and panic as programmer errors.

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
| `Task::process(spec, map_result)` | Shell-owned subprocess | Single command |
| `Task::process_interactive(spec, on_update)` | Shell-owned interactive subprocess | Incremental commands |
| `Task::blocking(|| ...)` | Non-abortable blocking work | Single command |
| `Task::blocking_cooperative(|cancel| ...)` | Lease-owned blocking work | `Option<Command>` |

### Decision Matrix (App Code)

Use this when deciding where new runtime work should live:

| You need | Use | Example | Anti-example |
|----------|-----|---------|--------------|
| Finite one-shot async work triggered by an event | `Command::effect(...)` + `Task::once(async { ... })` | Fetch profile after `Event::OpenProfile` | Modeling one request as a `Subscription` |
| Finite stream tied to explicit ownership | `Command::effect(...)` + `Task::stream(...)` (usually started through `AbortSlot`) | Typeahead stream while a search is active | Permanent `Subscription` for workflow-local stream |
| Shell-owned subprocess with one final result | `Task::process(...)` | `git status` + map exit/result to event | Spawning child process manually inside `Task::once` |
| Interactive subprocess with stdin control and streamed output | `Task::process_interactive(...)` + `AbortSlot::write/close_stdin` | REPL-like command session | Storing `Child` handles in model state |
| Long-lived source that should exist while model says so | `Subscription::every(...)` / `Subscription::custom(...)` | Tick while screen is visible; weather polling while enabled | Re-emitting timer effects each event loop step |
| Cancellation ownership in model state | `AbortSlot` (or explicit `TaskLease`) | Replace stale save job when user edits again | Fire-and-forget abortable commands without retaining owner |

Unhandled effects now fail fast by default. If an effect reaches the shell and no configured handler claims it, `step()` / `run()` returns `ShellError::UnhandledEffect`. Opt out explicitly with `SyzygyConfig::default().unhandled_effects(UnhandledEffectPolicy::Ignore)` when you really want legacy drop behavior.

```rust
fn run_git_status() -> Task<Event, Effect> {
    Task::process(
        ProcessSpec::new("git")
            .arg("status")
            .stdout(ProcessOutput::Capture { max_bytes: 8 * 1024 })
            .stderr(ProcessOutput::Capture { max_bytes: 8 * 1024 }),
        |result| match result {
            Ok(exit) => Command::event(Event::Finished(exit)),
            Err(error) => Command::event(Event::Failed(error)),
        },
    )
}
```

```rust
fn run_interactive(job: &mut SaveJob) -> Command<Event, Effect> {
    job.start(Effect::Echo)
        .and(job.write(b"hello\n"))
        .and(job.close_stdin())
}

fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    match effect {
        Effect::Echo => Task::process_interactive(
            ProcessSpec::new("sh")
                .args(["-c", "cat"])
                .stdin(ProcessInput::Piped)
                .stdout(ProcessOutput::Stream {
                    framing: ProcessFraming::Lines { max_line_bytes: 256 },
                }),
            |update| match update {
                ProcessUpdate::Stdout(frame) => Some(Command::event(Event::Stdout(frame))),
                ProcessUpdate::Exited(result) => Some(Command::event(Event::Finished(result))),
                ProcessUpdate::Stderr(_) => None,
            },
        ),
    }
}
```

## Subscriptions

Use `Task` for finite work. Use `Subscription` for long-lived event sources that should stay active while the model says they should exist.

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SubKey {
    Clock,
}

fn describe_subscriptions(running: &Running) -> Subscription<Event, Effect> {
    if !*running.get() {
        return Subscription::none();
    }

    Subscription::every(SubKey::Clock, Duration::from_secs(1), Event::Tick)
}

fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
    handle!(describe_subscriptions, ctx)
}
```

Register the handler on the builder:

```rust
let app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(handle_event)
    .subscription_handler(handle_subscriptions)
    .build()?;
```

For app-level integrations, register a custom driver and keep the handler pure:

```rust
let app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .with_subscription_driver(HttpPollDriver::new(client))
    .event_handler(handle_event)
    .subscription_handler(handle_subscriptions)
    .build()?;
```

`Subscription::custom::<Driver, _, _>(...)` describes the source. The driver owns the impure runtime work. Event/effect handlers never receive sender channels or runtime handles.

Common driver shapes can use built-in `SubscriptionDriver` helpers instead of hand-rolling `stream::unfold(...)` loops:

```rust
impl SubscriptionDriver for HttpPollDriver {
    type Spec = HttpPollSpec;
    type Update = HttpPayload;

    fn subscribe(&self, spec: Self::Spec) -> SubscriptionStream<Self::Update> {
        let every = spec.every;
        Self::poll_with(
            spec,
            every,
            SubscriptionPollStart::AfterInterval,
            |spec| async move { fetch_http_payload(spec.url).await },
        )
    }
}
```

Use `Self::poll(...)` for immediate-first polling and `Self::stream(stream)` when a backend already gives you a stream.

Custom drivers are constructed on Syzygy's owned runtime, so runtime-backed sources can be created safely inside `SubscriptionDriver::subscribe(...)`. If a driver should participate in deterministic manual time, prefer `syzygy::runtime::sleep(...)` over backend-specific timers like `tokio::time::interval(...)`.

## Command Composition

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

## Testing

```rust
use syzygy::prelude::*;

let mut store = TestStore::new(Model::default(), handle_event);
store.send(Event::Increment);

assert_eq!(store.state().counter, 1);
store.assert_effects([Effect::Save]);
```

**Note:** `emit()` is fire-and-forget (returns `()`). Use `try_send()` if you need to handle channel errors explicitly.

Use `RunnerTester` when the shell/runtime must stay real:

```rust
let mut tester = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(handle_event)
    .subscription_handler(handle_subscriptions)
    .build_tester()?;

tester.advance_time(Duration::from_secs(60));
tester.drain()?;
```

### Decision Matrix (Tests)

| Test question | Use | Example |
|---------------|-----|---------|
| Did pure event handlers update state and emit expected effects? | `TestStore` | `send(...)`, then `assert_state` / `assert_effects` |
| Does an effect map to the right completion events? | `TestStore::receive_async(...)` | Map `Effect::Save` to success/failure events |
| Do subscriptions, shell errors, process tasks, or manual time behave correctly? | `RunnerTester` | `advance_time(...)` + `drain()` + assert runtime outcome |
| Does full wiring (resources + drivers + config) work end-to-end? | Integration test with `Syzygy::builder()` | Build app like production and drive it with real events |

Live shell diagnostics are available without traces:

```rust
let snapshot = app.shell().snapshot();

assert_eq!(snapshot.pending_error_count, 0);
assert!(snapshot.last_termination.is_none());
```

`snapshot()` reports active tasks/subscriptions, deferred and queued work counts, and the latest explicit termination reason (`Completed`, `Failed`, or `Cancelled(...)` with a cancellation cause).

Structured traces are opt-in through diagnostics config:

```rust
let diagnostics = DiagnosticsConfig::default()
    .trace(ShellTraceConfig::default().enabled(true).max_entries(2_048));

let mut app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .event_handler(handle_event)
    .effect_handler(handle_effect)
    .with_syzygy_config(SyzygyConfig::default().diagnostics(diagnostics))
    .build()?;

app.step()?;
let trace = app.shell().trace_snapshot();
```

Trace entries are ordered and payload-light: they capture core/shell phases, command routing, task/subscription lifecycle, process control, and termination outcomes without requiring `Event`/`Effect` `Debug` bounds.

## Footguns

**Aliasing panics at runtime.** This will panic:

```rust
fn bad(ctx: &EventContext<Model>) {
    let _ = Model::extract(ctx);        // &Model
    let _ = Counter::extract_mut(ctx);  // &mut i32 inside Model
}
```

**Extraction panics are deliberate programmer errors.** Borrow overlap failures include the caller location and a hint to fix handler extractor overlap.

**Missing effect resources panic deliberately.** If an effect handler extracts an unregistered resource, Syzygy panics with the resource type, caller location, and a `.with_resource(...)` registration hint.

**Signature-based resource extraction clones on every effect access.** Expensive resources should be wrapped in `Arc<T>`, or borrowed explicitly with `ctx.resource_ref::<T>()` when you need non-cloning access:

```rust
#[derive(Clone)]
struct DbPool(Arc<Pool>);  // Cheap clone

// NOT: struct DbPool(Pool)  // Expensive clone every effect

fn handle_effect(effect: Effect, ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    let pool = ctx.resource_ref::<DbPool>().expect("DbPool must be registered");
    let _ = pool;
    match effect {
        Effect::Save => Task::none(),
    }
}
```

**256 field limit.** Exceeding 256 `#[model(...)]` fields fails during macro expansion.

**Abortable work needs an owner.** `AbortSlot` is the convenient state wrapper for the common case, but the underlying owner is still a `TaskLease`. If you schedule an abortable effect and do not retain its owner in model state, the shell will cancel it on the next `step`/`drain` cycle.

**Abortable command/task mapping is explicit.** Plain `Command::map` / `Task::map` reject abortable steps. Use `TaskLeaseScope` when you intentionally remap child abortable work into a parent domain.

**Blocking work is split on purpose.** `Task::blocking` is non-abortable and shutdown waits for it to finish. Lease-owned blocking work must use `Task::blocking_cooperative`; returning plain `Task::blocking` from an abortable effect is a `ShellError`.

**Process control is lease-addressed.** `Command::process_write`, `Command::process_close_stdin`, and the matching `AbortSlot` helpers only work while that lease still owns a live interactive process.

**Interactive process cancel is explicit and bounded.** The default process termination policy is `CloseStdinThenKill { grace: 500ms }`. If stdin is not piped, Syzygy skips straight to hard kill.

**Unmanaged subprocesses are outside Syzygy.** If you need shell-owned child-process cancellation on abort/shutdown, use `Task::process` or `Task::process_interactive` instead of spawning a child manually inside `Task::once` or `Task::blocking`.

## Owned Runtime

Syzygy owns the runtime it drives. By default the builder constructs it for you, but you can inject an owned runtime explicitly:

```rust
let runtime = syzygy::runtime::Runtime::new()?;

let app = Syzygy::builder::<Event, Effect>()
    .with_runtime(runtime)
    .model(Model::default())
    .event_handler(handle_event)
    .effect_handler(handle_effect)
    .build()?;
```

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
| `11_process_tasks` | `Task::process_interactive`, `AbortSlot`, runtime injection |
| `12_subscriptions` | Pure subscription handler, `Subscription::every`, `SubscriptionDriver::poll`, custom drivers |

## License

Unlicense. See `UNLICENSE`.
