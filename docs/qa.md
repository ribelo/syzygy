# QA

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
