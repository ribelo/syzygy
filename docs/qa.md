# QA

## 2026-05-13

Q: How should effect resources be represented after rejecting typed HLists and dynamic lookup?

A: Use one user-owned resources state type and pass it to the builder with `.resources(value)`. `EffectContext<'_, S>` borrows that state and exposes `ctx.state()`. Signature-based extraction stays available through explicit `FromEffectContext<S>` impls, plus the blanket `FromEffectContext<S> for S where S: Clone + 'static` for whole-state extraction.

Rejected alternatives:
- Dynamic type lookup: too much runtime machinery for wiring mistakes that should be visible in the app's own state type.
- Internal typed HLists: worse diagnostics, leaked framework marker types, and too much trait/macro surface.
- A derive macro in this slice: convenient, but not needed until hand-written extractors prove painful in real examples.

Reason: resources are a shell concern, not a model concern. A concrete state struct keeps ownership obvious and keeps missing resources as normal Rust type errors.

## 2026-05-02

Q: How should Syzygy verify public feature/package shapes after boundary repairs?

A: Keep the normal quality gate, and add a dedicated matrix command `./scripts/check-feature-matrix.sh` that checks `--no-default-features`, shell+runtime combinations, and a downstream derive/subscription fixture compile.

Reason: The quality gate validates default behavior, but boundary regressions frequently appear in non-default package shapes and downstream macro consumption.

## 2026-05-02

Q: How should mixed boot commands order event and non-event steps?

A: In the boot step, process boot `Command::event(...)` steps through Core before dispatching boot non-event steps.

Reason: Dispatching boot effects before boot event state updates created surprising startup ordering. This keeps boot deterministic and aligned with the chosen deferred command-event contract while preserving boot-before-external-event behavior.

## 2026-05-02

Q: What should shell "idle" mean for run/drain completion?

A: Treat idle as quiescent: no runtime activity, no routable deferred/queued work, and no pending shell errors. Expose this explicitly via `has_runtime_activity`, `has_routable_work`, `has_pending_errors`, and `is_quiescent`; `is_idle` delegates to `is_quiescent`.

Reason: Activity-only idle checks can report completion while deferred events or pending errors still exist.

## 2026-05-02

Q: How should deferred event backlog overflow behave?

A: Default to `DeferredEventOverflowPolicy::Error` and surface `ShellError::DeferredEventOverflow`. Allow lossy behavior only through explicit opt-in `DeferredEventOverflowPolicy::DropNewest`.

Reason: command-emitted events are part of state progression. Silent drop must not be the default policy.

## 2026-05-02

Q: What happens to model state when shell command dispatch fails?

A: Model mutations from the event handler remain committed. Syzygy does not roll back state on `ShellError`; errors are surfaced for recovery.

Reason: core event handling and model updates occur before shell command routing.

## 2026-05-02

Q: Are shell traces a deterministic replay input?

A: No. `ShellTraceEntry` is diagnostics-only in this epic: ordered, payload-light observability for debugging and regressions, not a replay log contract.

Reason: keep diagnostics useful without implying deterministic reconstruction guarantees. Deterministic replay/time-travel remains separate follow-up work in `syzygy-b7e.3`.

## 2026-05-02

Q: How should mutable core/shell/model access APIs be framed?

A: Keep them as explicit advanced escape hatches for integration/tests, and document invariants callers must preserve. Normal app flow remains event-driven (`try_send` + `step`/`run`) with shell observation via snapshot/trace APIs.

Reason: preserve power-user integration paths without weakening the default boundary contract.

## 2026-05-02

Q: What runtime contract does `TestStore::receive_async` provide?

A: `receive_async` is async-callable convenience for tests already in async contexts, but it is still TestStore-driven effect resolution (internal runtime/block_on path), not caller-runtime orchestration. It intentionally rejects shell-owned process tasks; use `RunnerTester` for shell/runtime lifecycle behavior.

Reason: keep TestStore focused on effect-result/state assertions and avoid implying full runtime/process modeling.

## 2026-05-02

Q: Which public error types should Syzygy expose after boundary cleanup?

A: Keep the public error surface focused on `CoreError` and `ShellError`; remove stale standalone `CommandError` / `EffectError` types that are not owned by any active runtime path.

Reason: fewer error types with clear ownership make boundary contracts easier to understand and maintain.

## 2026-05-02

Q: How should we teach "generic core / specific shell" without adding new framework traits?

A: Prefer docs + runnable examples first. Keep reusable feature cores as plain model/event/effect modules and show explicit parent mapping into app-specific shell behavior.

Reason: reinforces the boundary contract without reintroducing heavyweight abstractions.

## 2026-04-18

Q: How should Syzygy describe runtime cost and extraction safety guarantees?

A: Do not claim "zero-overhead". Document runtime costs explicitly: effect resource injection clones by default, extractor aliasing is enforced at runtime, and violations panic as programmer errors.

Reason: The runtime intentionally does real work (task scheduling, resource cloning, borrow tracking). Marketing or docs that imply compile-time-only guarantees are inaccurate and make failure modes harder to reason about.

## 2026-04-18

Q: How should production code bound `run_until` waits?

A: Keep `run_until(...)` for unbounded behavior, and add explicit bounded/cancellable variants: `run_until_timeout(...)`, `run_until_deadline(...)`, and `run_until_or_cancelled(...)` returning `RunUntilExit`.

Reason: Test-only timeout helpers are not enough for production loops. Bounded waits and cooperative cancellation keep liveness behavior explicit and reuse one timeout error surface (`ShellError::Timeout`).

## 2026-03-08

Q: How should `syzygy::runtime::yield_now()` behave under `rt-compio`?

A: Use a one-shot self-wake future built with `std::future::poll_fn`, not `compio::runtime::time::sleep(Duration::ZERO)`.

Reason: In `compio-runtime 0.11`, `sleep` is implemented as `sleep_until(Instant::now() + duration)`, and zero-duration deadlines skip timer registration. That makes `sleep(Duration::ZERO)` complete immediately instead of cooperatively yielding. The self-wake future requeues the task and lets other ready tasks run before the current task resumes.

## 2026-03-08

Q: Should `Shell` or `Syzygy` expose async driving APIs?

A: No. `Core` and `Shell` progress synchronously, but `Syzygy` owns the configured async runtime and drives it internally through sync `step()` / `run_until()`.

Reason: Public async stepping leaks runtime policy into the shell boundary and makes external async driving part of the API. The shell should route ready work synchronously while effect tasks stay async internally, and it must not depend on an ambient runtime or create fallback runtimes in execution paths.

## 2026-03-08

Q: How should abortable work be modeled?

A: Use lease-owned abortable effects, not raw cancel IDs and not imperative runtime handles.

Reason: `Effect` must stay a description, but raw `track/cancel` IDs make it easy for the app to lose ownership knowledge while the shell still runs background work. `TaskLease` keeps ownership explicit in app state, lets commands stay declarative (`Command::abortable` / `Command::cancel`), and allows the shell to cancel work safely when explicit cancel, replacement, owner loss, or shutdown happens. Mapping abortable commands/tasks across boundaries must stay explicit too: plain `Command::map` / `Task::map` reject abortable steps, and `TaskLeaseScope` is required when a caller intentionally remaps child abortable work into a parent domain.

## 2026-03-08

Q: How should blocking work fit the lease-owned abortable task model?

A: Split it explicitly. `Task::blocking` is non-abortable blocking work, while lease-owned blocking work must use `Task::blocking_cooperative` with a `BlockingCancelToken`.

Reason: Runtime blocking threads cannot honestly support hard abort once started. Pretending otherwise would make `Command::abortable` lie about what cancellation means. The correct contract is: plain blocking work runs to completion and shutdown waits for it, while abortable blocking work must cooperatively observe cancellation and return `None` when cancelled. If an abortable effect resolves to plain `Task::blocking`, the shell returns `ShellError::AbortableBlockingTask` instead of silently accepting dishonest semantics.

## 2026-03-09

Q: What should `#[derive(Model)]` expose by default?

A: Only whole-model extraction. Field extraction is opt-in per field via `#[model(wrapper = Name)]` or `#[model(part)]`.

Reason: Implicit field wrappers create public API surface from naming conventions instead of explicit declarations. That conflicts with Syzygy's goal of making model access obvious at the declaration site. The derive should not invent extractor names or field-level access unless the field explicitly opts in.

## 2026-03-09

Q: How should Syzygy improve abortable task DX without hiding ownership?

A: Add `AbortSlot` as an explicit model-state helper on top of `TaskLease`.

Reason: The low-level lease API is correct but repetitive for the common "one piece of model state owns one abortable task" case. `AbortSlot` keeps ownership in model state, builds declarative commands, and stays honest about replacement semantics by emitting `cancel(previous)` before `abortable(next, effect)`. The runtime still only sees `TaskLease` and `Command` descriptions.

## 2026-03-09

Q: How should child processes fit the explicit effect model?

A: Add a first-class `Task::process(ProcessSpec, map_result)` API and keep subprocess lifecycle shell-owned.

Reason: Spawning `std::process::Command` or backend-specific child handles inside arbitrary user futures makes subprocess ownership invisible to the shell, which breaks the "no orphaned process" rule. `ProcessSpec` keeps the effect declarative, `Task::process` keeps backend details out of the public API, and cancellation stays honest because the spawned child is killed when the shell drops the task on cancel, owner loss, replacement, or shutdown. Output capture must stay bounded at the call site.

## 2026-03-09

Q: How should callers customize runtime ownership?

A: Keep runtime ownership inside Syzygy, but let the builder accept an already-owned `syzygy::runtime::Runtime` through `with_runtime(runtime)`.

Reason: An injected runtime is still explicit ownership transfer into Syzygy. This avoids ambient-runtime coupling, avoids backend-specific builder entry points, and keeps all shell execution on one owned runtime instance while still letting advanced callers choose when and how that runtime is constructed.

## 2026-03-09

Q: How should interactive subprocesses work without breaking the explicit effect model?

A: Keep the child process shell-owned with `Task::process_interactive`, stream `ProcessUpdate` values back into commands, and address stdin control through lease-owned commands (`Command::process_write` / `Command::process_close_stdin`).

Reason: Interactive child handles are runtime state, not effect descriptions, so they cannot live in user model code. `Task::process_interactive` keeps the effect declarative while still allowing incremental stdout/stderr updates and explicit stdin writes. Stdin control must stay lease-addressed so ownership remains visible in model state and cancellation semantics stay identical to other abortable work.

## 2026-03-09

Q: What is the default cancellation policy for shell-owned processes?

A: Use `CloseStdinThenKill { grace: 500ms }` by default, but kill immediately when stdin is not piped.

Reason: Closing stdin first is the most portable graceful shutdown signal Syzygy can own across backends today. Waiting for grace when there is no piped stdin would be dishonest because the shell has no cooperative signal to send, so the correct behavior there is immediate hard kill.

## 2026-03-09

Q: Should stale compatibility aliases and superseded tracker entries stay around after the lease/runtime redesign?

A: No. Remove stale compatibility shims like `assert_tracked_effect` / `assert_slot_empty`, rename tests to the current abortable/lease terminology, and close superseded or already-implemented beads.

Reason: Keeping two names for one concept makes the repository lie about the current API surface. Syzygy’s design goal is explicitness, so cleanup is not optional polish here; the code, docs, tests, and tracker should all use the same vocabulary.

## 2026-03-09

Q: How should Syzygy model long-lived external event sources without giving app code live channels or handles?

A: Add a separate pure `Subscription` mechanism. The app returns a state-derived `Subscription<Event, Effect>` from a read-only `SubscriptionContext`, and the shell diffs desired subscriptions against active runtime-owned sources.

Reason: Long-lived timers, watchers, and polling loops are not finite `Task`s, but handing a sender/channel into app code would break the rule that effects stay descriptive. `Subscription` keeps the app layer pure: the model describes which sources should exist, the shell owns lifecycle by explicit key, and custom integrations live behind registered `SubscriptionDriver`s instead of imperative backchannels.

## 2026-03-09

Q: How should subscription handlers and custom driver construction interact with shell/runtime ownership?

A: Keep the model type attached to `Shell`, and construct `SubscriptionDriver::subscribe(...)` inside the owned runtime task, not in synchronous reconciliation code.

Reason: Erasing the model type inside `Shell` makes safe `Core`/`Shell` recombination unsound because a shell built for one model can be paired with another. Separately, many runtime-backed drivers need an active tokio/compio runtime at construction time, so the shell must only validate registration synchronously and defer actual stream construction until the spawned subscription task is polled on Syzygy's owned runtime.

## 2026-03-09

Q: Which remaining lifecycle/tooling ideas should Syzygy pull next from iced/crux?

A: Prioritize an explicit boot hook, a shell-level `RunnerTester`, and deterministic clock plus trace/replay tooling. Do not add a view layer.

Reason: These three features strengthen Syzygy's existing runtime model without weakening the rule that effects stay descriptive. A view abstraction is not needed for Syzygy's current goals and would add a second architectural axis before the runtime/testing story is complete.

## 2026-03-09

Q: How should Syzygy handle unhandled effects and missing effect resources by default?

A: Fail fast by default. Final unhandled effects must surface as `ShellError::UnhandledEffect`, and missing effect resources must panic through one `#[track_caller]` helper with a registration hint.

Reason: Silently dropping an effect lies about what the shell did. Missing resources are also developer errors, but today they fail through rough ad hoc panics. The correct first step is not a broad handler redesign; it is a stricter default and one precise failure path. Permissive unhandled-effect behavior may still exist, but only as an explicit opt-out in `SyzygyConfig`.

## 2026-03-09

Q: How should Syzygy run startup work?

A: Add an explicit one-shot `boot_handler` on the builder. It receives `&Model`, returns a normal `Command<Event, Effect>`, runs exactly once, and executes before the first ordinary event drain and before subscription reconciliation.

Reason: Fake startup events pushed from outside the runtime hide ordering and make the first step depend on channel timing. A builder-level boot hook keeps startup declarative, keeps the command model uniform, and makes first-step ordering explicit: boot command, core event processing, subscription reconciliation, shell drain.

## 2026-03-09

Q: How should Syzygy make time deterministic for runtime-owned tasks and subscriptions?

A: Move `syzygy::runtime::sleep(...)` onto an owned clock abstraction and add `Runtime::manual()` for tests. The active clock is bound by the owned runtime when it polls tasks, so existing task/subscription code keeps using `syzygy::runtime::sleep(...)` while tests can advance time explicitly with a returned `ManualClock`.

Reason: Wall-clock timers block deterministic tests and make future `RunnerTester` time control impossible. Binding sleep to the owned runtime keeps time as runtime infrastructure instead of an app-layer capability, makes `Subscription::every(...)` deterministic, and keeps custom drivers pure as long as they use Syzygy's sleep helper instead of backend-specific timer APIs.

## 2026-03-09

Q: What should `RunnerTester` own, and where does it stop?

A: `RunnerTester` should own a real `Syzygy` runner built with a manual runtime clock, expose bounded `step()` / `drain()` helpers, explicit `advance_time(...)`, direct state assertions, and raw shell errors from `step()`. It should not emulate shell behavior, should not replace `TestStore`, and should not hide the underlying runner when a test needs lower-level access.

Reason: The testing gap is specifically about shell-owned behavior that `TestStore` cannot model honestly. A thin wrapper over a real runner keeps lifecycle semantics truthful, reuses the new deterministic clock, and avoids inventing a second fake runtime model. `TestStore` remains the right tool for pure event/effect logic; `RunnerTester` exists for subscriptions, shell errors, runtime-owned async work, and processes.

## 2026-03-09

Q: How should direct test helpers interact with `syzygy::runtime::sleep(...)` and long waits?

A: Keep `runtime::sleep(...)` bound to Syzygy runtime contexts, but make `TestStore::receive` / `receive_async` drive future and stream tasks under a temporary owned `Runtime` so pure effect tests still work. Separately, `RunnerTester::wait_for(...)` must honor the caller timeout instead of imposing an earlier iteration cap whenever a real timeout is present.

Reason: `TestStore` is still a supported public harness for pure effect tests, so it must not regress just because tasks started using Syzygy's runtime helper. At the same time, `RunnerTester` should stay honest: iteration caps are safety rails for unbounded waits, not a hidden replacement for the explicit timeout the test author asked for.

## 2026-03-09

Q: After the core runtime model stabilized, which polish work should Syzygy prioritize next?

A: Prioritize four things: live shell introspection, extraction ergonomics, clearer choice guidance, and lower-boilerplate subscription drivers. Keep the current explicit and pure runtime boundary; do not add sender-based worker APIs or hidden background handles.

Reason: The remaining weaknesses are mostly visibility and product polish, not architecture. `Trace` and replay tooling explain behavior after the fact, but day-to-day debugging also needs a cheap `ShellSnapshot`-style view of active tasks, subscriptions, and termination reasons. Explicit extraction is correct but still noisier than it should be for common fields, so wrapper ergonomics and macro warning hygiene remain worthwhile. The choice between `Task`, `Subscription`, process tasks, and test harnesses is now sound but still not taught clearly enough, and custom subscription drivers still involve more glue than necessary. The right direction is to reduce friction without weakening the rule that effects are descriptions and runtime capabilities stay out of app code.

## 2026-03-09

Q: How should live shell introspection expose teardown outcomes without trace payloads?

A: Add `Shell::snapshot()` as a cheap observational API and track one explicit `last_termination` record with target (`Task`/`Process`/`Subscription`) plus reason (`Completed`, `Failed`, `Cancelled(...)`). Keep cancellation causes explicit (`ExplicitCommand`, `Replacement`, `OwnerDropped`, `SubscriptionReconciled`, `Shutdown`).

Reason: Day-to-day debugging needs immediate lifecycle visibility even when trace recording is disabled. A small fixed reason taxonomy gives actionable teardown context without exposing runtime internals or requiring `Event`/`Effect` debug bounds.

## 2026-03-09

Q: How should structured trace recording be exposed without mutating handler APIs or requiring `Debug` on app event/effect types?

A: Add an opt-in shell-owned recorder configured through `DiagnosticsConfig.trace(ShellTraceConfig { ... })`. Record ordered, payload-light `ShellTraceEntry` transitions (core/shell phases, command routing, task/subscription/process lifecycle, termination) with bounded retention (`max_entries` ring buffer).

Reason: Trace capture must stay runtime/test infrastructure, not an app-layer concern. A typed transition stream with sequence numbers gives deterministic replay input and practical debugging context while preserving explicit effect boundaries and keeping default overhead near zero when tracing is disabled.

## 2026-03-09

Q: How should wrapper ergonomics improve without reintroducing implicit extraction?

A: Keep `#[model(wrapper = ...)]` explicit, but generate small helper methods on wrappers (`get`, `get_mut`, `set`, `replace`) and, for `Option<T>` wrappers, explicit option helpers (`is_some`, `as_ref`, `insert`, `get_or_insert`, `take`, `clear`).

Reason: The explicit extraction boundary is correct, but common wrapper usage should not require noisy deref patterns. Helper methods reduce boilerplate while keeping ownership and extraction intent visible in handler code.

## 2026-03-09

Q: How should `#[derive(Model)]` avoid downstream `unexpected_cfgs` noise for subscription impls?

A: Do not emit downstream-local `#[cfg(feature = "shell")]` around generated `SubscriptionPart` impls. Keep `SubscriptionPart`/`SubscriptionContext` available in the extraction layer, and gate shell-owned runtime subscription behavior separately.

Reason: Downstream `cfg(feature = "shell")` is evaluated against the downstream crate, not Syzygy's dependency feature graph, so it can silently remove required impls. Making extraction traits always available preserves derive correctness and avoids both warning-noise and missing-impl bugs.

## 2026-03-09

Q: How should Syzygy teach the choice between effect/task/process/subscription primitives and between `TestStore` vs `RunnerTester`?

A: Use a lifecycle-first decision matrix with concrete examples and anti-examples. Keep finite event-triggered work on `Command::effect + Task::{once,stream,process,process_interactive}`, keep long-lived state-derived sources on `Subscription`, keep cancellation ownership explicit via `AbortSlot`/`TaskLease`, and mirror the same rule in test docs (`TestStore` for pure logic, `RunnerTester` for shell/runtime behavior).

Reason: Most remaining confusion is about where work should live, not missing runtime capability. A short matrix prevents category mistakes (for example one-shot requests modeled as subscriptions, or subprocesses spawned outside shell ownership) while preserving Syzygy's explicit runtime boundary.

## 2026-03-09

Q: How should Syzygy reduce custom `SubscriptionDriver` boilerplate for polling and stream-backed sources without weakening runtime ownership boundaries?

A: Keep `SubscriptionDriver` explicit, but add small default adapters on the trait itself: `stream(...)` for prebuilt streams, `poll(...)` for immediate-first periodic polling, and `poll_with(..., SubscriptionPollStart, ...)` when the first poll should wait for the interval.

Reason: The friction is repetitive stream plumbing, not a missing abstraction. Trait-level helpers cut boilerplate while preserving the existing model: specs stay pure data, drivers are still registered explicitly, and app handlers still never receive sender/runtime handles.

## 2026-03-09

Q: How should Syzygy handle extraction failures that are still programmer errors while improving diagnostics?

A: Keep model extraction overlap/capacity failures as deliberate panics with consistent programmer-error hints. Effect resource wiring no longer uses dynamic lookup; missing effect resources should be represented by ordinary compile errors on the user-owned resources state type or missing `FromEffectContext<AppResources>` impls.

Reason: Model borrow violations are runtime aliasing bugs. Effect resource wiring is static app state and should not need a runtime registration table.

## 2026-03-09

Q: How should Syzygy avoid false-positive Option helper generation and shutdown snapshot target overwrite regressions?

A: Restrict derive-time option-helper detection to known std/core `Option` paths (`Option<T>`, `std::option::Option<T>`, `core::option::Option<T>`), and only emit generic untracked-task shutdown termination records when no specific active-task/subscription shutdown record was emitted earlier in the same shutdown pass.

Reason: Path-suffix matching on `...::Option<T>` is too broad and breaks custom types named `Option`. Separately, generic untracked shutdown records were overwriting more precise `Subscription`/`Process` targets in `last_termination`, making snapshot diagnostics lie about what was actually shut down.
