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
