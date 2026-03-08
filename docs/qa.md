# QA

## 2026-03-08

Q: How should `syzygy::runtime::yield_now()` behave under `rt-compio`?

A: Use a one-shot self-wake future built with `std::future::poll_fn`, not `compio::runtime::time::sleep(Duration::ZERO)`.

Reason: In `compio-runtime 0.11`, `sleep` is implemented as `sleep_until(Instant::now() + duration)`, and zero-duration deadlines skip timer registration. That makes `sleep(Duration::ZERO)` complete immediately instead of cooperatively yielding. The self-wake future requeues the task and lets other ready tasks run before the current task resumes.

## 2026-03-08

Q: Should `Shell` or `Syzygy` expose async driving APIs?

A: No. `Core` and `Shell` progress synchronously. Async is confined to effect execution, and Syzygy owns progression through sync `step()` / `run_until()`.

Reason: Public async stepping leaks runtime policy into the shell boundary and makes external async driving part of the API. The shell should route ready work synchronously while effect tasks stay async internally.
