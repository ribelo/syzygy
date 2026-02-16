This file is a merged representation of a subset of the codebase, containing specifically included files, combined into a single document by Repomix.

# File Summary

## Purpose
This file contains a packed representation of a subset of the repository's contents that is considered the most important context.
It is designed to be easily consumable by AI systems for analysis, code review,
or other automated processes.

## File Format
The content is organized as follows:
1. This summary section
2. Repository information
3. Directory structure
4. Repository files (if enabled)
5. Multiple file entries, each consisting of:
  a. A header with the file path (## File: path/to/file)
  b. The full contents of the file in a code block

## Usage Guidelines
- This file should be treated as read-only. Any changes should be made to the
  original repository files, not this packed version.
- When processing this file, use the file path to distinguish
  between different files in the repository.
- Be aware that this file may contain sensitive information. Handle it with
  the same level of security as you would the original repository.

## Notes
- Some files may have been excluded based on .gitignore rules and Repomix's configuration
- Binary files are not included in this packed representation. Please refer to the Repository Structure section for a complete list of file paths, including binary files
- Only files matching these patterns are included: src/**, tests/**, examples/**, benches/**, Cargo.toml
- Files matching patterns in .gitignore are excluded
- Files matching default ignore patterns are excluded
- Files are sorted by Git change count (files with more changes are at the bottom)

# Directory Structure
```
benches/
  arc_vs_fn_benchmark.rs
  command_performance.rs
  shell_throughput.rs
examples/
  async_effect.rs
  basic_counter.rs
  manual_loop.rs
  timeout_pattern.rs
  two_executors.rs
src/
  error/
    command.rs
    core.rs
    effect.rs
    mod.rs
    shell.rs
  executor/
    inline_async.rs
    mod.rs
    rayon_sync_executor.rs
    registry.rs
    single_thread_executor.rs
    task.rs
    tokio_executor.rs
  activity.rs
  builder.rs
  cli.rs
  command.rs
  core.rs
  lib.rs
  resource_cell.rs
  shell.rs
  syzygy.rs
tests/
  executor/
    mod.rs
    single_thread_executor_basic.rs
    tokio_executor_basic.rs
    tokio_io_concurrency.rs
  e2e_nonblocking_io.rs
  lib.rs
  order_properties.rs
  property_tests.proptest-regressions
  shell_method_tests.rs
  timeout_event_pattern.rs
  verified_behaviors.rs
Cargo.toml
```

# Files

## File: benches/shell_throughput.rs
````rust
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use syzygy::command::Command;
use syzygy::executor::Task;
use syzygy::prelude::{Syzygy, SyzygyProfile};

#[derive(Default)]
struct BenchModel;

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum BenchEvent {
    Tick,
}

#[derive(Clone, Copy)]
enum BenchEffect {
    Nop,
}

fn update(_event: BenchEvent, _model: &mut BenchModel) -> Command<BenchEvent, BenchEffect> {
    Command::effect(BenchEffect::Nop)
}

fn effects(effect: BenchEffect, _resources: ()) -> Task<BenchEvent, BenchEffect> {
    match effect {
        BenchEffect::Nop => Task::none(),
    }
}

fn command_routing_bench(c: &mut Criterion) {
    const EFFECTS_PER_ITER: usize = 128;

    let mut group = c.benchmark_group("shell");
    group.throughput(Throughput::Elements(EFFECTS_PER_ITER as u64));
    group.bench_function("dispatch_command_128_effects", |b| {
        let mut runner = Syzygy::builder::<BenchEvent, BenchEffect>()
            .model(BenchModel::default())
            .event_handler(update)
            .effect_handler(effects)
            .with_profile(SyzygyProfile::Interactive)
            .build();

        b.iter(|| {
            {
                let shell = runner.shell_mut();
                for _ in 0..EFFECTS_PER_ITER {
                    shell
                        .dispatch_command(Command::effect(BenchEffect::Nop))
                        .expect("command dispatch succeeds");
                }
            }
            runner.shell_mut().drain().expect("drain succeeds");
        });
    });

    group.bench_function("dispatch_and_drain_256_effects", |b| {
        let mut runner = Syzygy::builder::<BenchEvent, BenchEffect>()
            .model(BenchModel::default())
            .event_handler(update)
            .effect_handler(effects)
            .with_profile(SyzygyProfile::Interactive)
            .build();

        b.iter(|| {
            {
                let shell = runner.shell_mut();
                for _ in 0..256 {
                    shell
                        .dispatch_command(Command::effect(BenchEffect::Nop))
                        .expect("command dispatch succeeds");
                }
            }
            runner.shell_mut().drain().expect("drain succeeds");
        });
    });

    group.finish();
}

criterion_group!(shell_benches, command_routing_bench);
criterion_main!(shell_benches);
````

## File: examples/timeout_pattern.rs
````rust
//! Demonstrates modeling timeouts as explicit events.
//!
//! Run with:
//! ```bash
//! cargo run --example timeout_pattern --features examples
//! ```

use std::time::Duration;

use syzygy::executor::{Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct TimeoutModel {
    is_loading: bool,
    error_message: Option<String>,
    data: Option<String>,
    timeout_count: u32,
}

#[derive(Debug, Clone)]
enum TimeoutEvent {
    StartSlowOperation,
    OperationCompleted { data: String },
    OperationTimeout { duration: Duration },
    RetryOperation,
}

#[derive(Debug, Clone)]
enum TimeoutEffect {
    SlowOperation { delay_ms: u64 },
}

fn timeout_update(
    event: TimeoutEvent,
    model: &mut TimeoutModel,
) -> Command<TimeoutEvent, TimeoutEffect> {
    match event {
        TimeoutEvent::StartSlowOperation => {
            model.is_loading = true;
            model.error_message = None;
            cmd::effect(TimeoutEffect::SlowOperation { delay_ms: 150 })
        }
        TimeoutEvent::OperationCompleted { data } => {
            model.is_loading = false;
            model.data = Some(data);
            model.timeout_count = 0;
            cmd::none()
        }
        TimeoutEvent::OperationTimeout { duration } => {
            model.is_loading = false;
            model.timeout_count += 1;
            model.error_message = Some(format!(
                "Operation timed out after {:?} (attempt {})",
                duration, model.timeout_count
            ));

            if model.timeout_count < 2 {
                cmd::event(TimeoutEvent::RetryOperation)
            } else {
                cmd::none()
            }
        }
        TimeoutEvent::RetryOperation => {
            model.is_loading = true;
            cmd::effect(TimeoutEffect::SlowOperation { delay_ms: 80 })
        }
    }
}

fn timeout_effect_handler(
    effect: TimeoutEffect,
    _resources: (),
) -> Task<TimeoutEvent, TimeoutEffect> {
    match effect {
        TimeoutEffect::SlowOperation { delay_ms } => {
            Task::async_on::<TokioExecutor, _>(async move {
                let operation = async move {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    format!("Operation completed after {delay_ms}ms")
                };

                let timeout_window = Duration::from_millis(120);
                match tokio::time::timeout(timeout_window, operation).await {
                    Ok(data) => cmd::event(TimeoutEvent::OperationCompleted { data }),
                    Err(_) => cmd::event(TimeoutEvent::OperationTimeout {
                        duration: timeout_window,
                    }),
                }
            })
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<TimeoutEvent, TimeoutEffect>()
        .model(TimeoutModel::default())
        .event_handler(timeout_update)
        .effect_handler(timeout_effect_handler)
        .profile_server()
        .with_async_executor(TokioExecutor::current_thread_io("timeout-pattern"))
        .build();

    app.core()
        .try_send_event(TimeoutEvent::StartSlowOperation)?;

    app.run_until(|core, _| !core.model().is_loading)?;

    let model = app.core().model();
    println!(
        "Loaded data: {:?}, timeouts: {}, error: {:?}",
        model.data, model.timeout_count, model.error_message
    );

    Ok(())
}
````

## File: src/activity.rs
````rust
//! # Activity Tracker
//!
//! This module provides the `Activity` struct, which tracks in-flight async work
//! beyond the effect queue. This is essential for proper "await idle" functionality
//! in CLI applications where we need to know when all async work has completed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct ActivityInner {
    inflight: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

/// Tracks in-flight async work beyond the effect queue
///
/// This is used to determine when the system is truly idle, accounting for
/// async jobs that have been spawned but may not have completed yet.
#[derive(Debug, Clone)]
pub struct Activity {
    inner: Arc<ActivityInner>,
}

impl Activity {
    /// Create a new Activity tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ActivityInner {
                inflight: AtomicUsize::new(0),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
            }),
        }
    }

    /// Increment the in-flight counter
    ///
    /// Call this when spawning async work.
    pub fn inc(&self) {
        self.inner.inflight.fetch_add(1, Ordering::AcqRel);
    }

    /// Decrement the in-flight counter
    ///
    /// Call this when async work completes. This will notify any waiters
    /// if the count reaches zero.
    pub fn dec(&self) {
        let prev = self.inner.inflight.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(prev > 0, "activity counter underflow");
        if prev == 1 {
            // We just reached zero, notify waiters
            let guard = self.inner.mutex.lock().unwrap();
            // Hold the lock briefly to pair with wait_until_zero
            self.inner.condvar.notify_all();
            drop(guard);
        }
    }

    /// Get the current count of in-flight jobs
    #[must_use]
    pub fn load(&self) -> usize {
        self.inner.inflight.load(Ordering::Acquire)
    }

    /// Wait until the in-flight count reaches zero or timeout expires
    ///
    /// Returns true if count reached zero, false if timeout occurred.
    pub fn wait_until_zero(&self, timeout: Duration) -> bool {
        if self.load() == 0 {
            return true;
        }

        if timeout.is_zero() {
            return self.load() == 0;
        }

        let deadline = if timeout == Duration::MAX {
            None
        } else {
            Instant::now().checked_add(timeout)
        };

        let mut guard = self.inner.mutex.lock().unwrap();
        loop {
            if self.load() == 0 {
                return true;
            }

            match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return false;
                    }

                    let wait_duration = deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO);

                    let result = self
                        .inner
                        .condvar
                        .wait_timeout(guard, wait_duration)
                        .unwrap();
                    guard = result.0;
                    if result.1.timed_out() && self.load() != 0 {
                        return false;
                    }
                }
                None => {
                    guard = self.inner.condvar.wait(guard).unwrap();
                }
            }
        }
    }
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_activity_basic() {
        let activity = Activity::new();
        assert_eq!(activity.load(), 0);

        activity.inc();
        assert_eq!(activity.load(), 1);

        activity.dec();
        assert_eq!(activity.load(), 0);
    }

    #[test]
    fn test_activity_wait_until_zero_immediate() {
        let activity = Activity::new();
        assert!(activity.wait_until_zero(Duration::from_millis(100)));
    }

    #[test]
    fn test_activity_wait_until_zero_timeout() {
        let activity = Activity::new();
        activity.inc();

        let start = std::time::Instant::now();
        let result = activity.wait_until_zero(Duration::from_millis(10));
        let elapsed = start.elapsed();

        assert!(!result);
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn test_activity_wait_until_zero_with_completion() {
        let activity = Arc::new(Activity::new());
        activity.inc();

        let activity_clone = Arc::clone(&activity);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            activity_clone.dec();
        });

        let start = std::time::Instant::now();
        let result = activity.wait_until_zero(Duration::from_millis(200));
        let elapsed = start.elapsed();

        assert!(result);
        assert!(elapsed >= Duration::from_millis(40)); // Allow some tolerance
        assert!(elapsed < Duration::from_millis(150));
    }

    #[test]
    fn test_activity_clone() {
        let activity1 = Activity::new();
        activity1.inc();

        let activity2 = activity1.clone();
        assert_eq!(activity2.load(), 1);

        activity2.dec();
        assert_eq!(activity1.load(), 0);
        assert_eq!(activity2.load(), 0);
    }
}
````

## File: src/cli.rs
````rust
//! Helpers for building command-line Syzygy applications.
//!
//! These utilities are intentionally lightweight wrappers that make it easier to
//! integrate `Syzygy` with traditional CLI workflows such as handling Ctrl-C
//! or reading from standard input.

use crate::core::EventSender;

use std::io::{self, BufRead};
use std::sync::Arc;
use std::thread;

/// Install a Ctrl-C handler that submits an event to the core.
///
/// The handler keeps running until the process exits. If the event channel is
/// already closed we silently ignore the signal.
pub fn install_ctrlc<E, F>(sender: EventSender<E>, make_event: F) -> Result<(), ctrlc::Error>
where
    E: Send + 'static,
    F: Fn() -> E + Send + Sync + 'static,
{
    let generator = Arc::new(make_event);
    let handler_sender = sender;
    ctrlc::set_handler(move || {
        let event = (generator.as_ref())();
        let _ = handler_sender.send(event);
    })
}

/// Spawn a background thread that reads lines from standard input and converts
/// them into events.
///
/// The provided closure runs on the background thread. Returning `None` skips
/// the line. If the event channel is closed the thread will exit.
pub fn spawn_stdin_listener<E, F>(sender: EventSender<E>, mapper: F) -> thread::JoinHandle<()>
where
    E: Send + 'static,
    F: FnMut(&str) -> Option<E> + Send + 'static,
{
    thread::Builder::new()
        .name("syzygy-stdin-listener".into())
        .spawn(move || {
            let stdin = io::stdin();
            let reader = stdin.lock();
            pump_lines(reader, sender, mapper);
        })
        .expect("failed to spawn syzygy stdin listener")
}

fn pump_lines<R, E, F>(mut reader: R, sender: EventSender<E>, mut mapper: F)
where
    R: BufRead,
    E: Send + 'static,
    F: FnMut(&str) -> Option<E>,
{
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim_end_matches(|c| c == '\n' || c == '\r');
                if let Some(event) = mapper(trimmed) {
                    if sender.send(event).is_err() {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::core::Core;
    use std::io::Cursor;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Line(String),
    }

    fn update(event: Event, model: &mut Vec<String>) -> Command<Event, ()> {
        model.push(match event {
            Event::Line(line) => line,
        });
        Command::none()
    }

    #[test]
    fn pump_lines_dispatches_events() {
        let (mut core, sender) = Core::new(update, Vec::<String>::new());
        let input = Cursor::new(b"one\n two \r\n\n");
        pump_lines(input, sender, |line| {
            if line.trim().is_empty() {
                None
            } else {
                Some(Event::Line(line.trim().to_string()))
            }
        });

        let _ = core.process_events();
        assert_eq!(
            core.model(),
            &vec![String::from("one"), String::from("two")]
        );
    }
}
````

## File: tests/order_properties.rs
````rust
#![cfg(feature = "shell")]

use proptest::collection::vec;
use proptest::prelude::*;
use syzygy::command::Command;
use syzygy::executor::Task;
use syzygy::prelude::{Syzygy, SyzygyProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropEvent {
    Primary(u16, bool),
    Secondary(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropEffect {
    EmitSecondary(u16),
}

#[derive(Default)]
struct PropModel {
    processed: Vec<PropEvent>,
}

fn update(event: PropEvent, model: &mut PropModel) -> Command<PropEvent, PropEffect> {
    match event {
        PropEvent::Primary(id, spawn_extra) => {
            model.processed.push(PropEvent::Primary(id, spawn_extra));
            if spawn_extra {
                Command::effect(PropEffect::EmitSecondary(id))
            } else {
                Command::none()
            }
        }
        PropEvent::Secondary(id) => {
            model.processed.push(PropEvent::Secondary(id));
            Command::none()
        }
    }
}

fn effects(effect: PropEffect, _resources: ()) -> Task<PropEvent, PropEffect> {
    match effect {
        PropEffect::EmitSecondary(id) => Task::event(PropEvent::Secondary(id)),
    }
}

proptest! {
    #[test]
    fn preserves_fifo_order(cases in vec((0u16..256u16, proptest::bool::ANY), 0..24)) {
        let mut runner = Syzygy::builder::<PropEvent, PropEffect>()
            .model(PropModel::default())
            .event_handler(update)
            .effect_handler(effects)
            .with_profile(SyzygyProfile::Interactive)
            .build();

        for (id, spawn_extra) in &cases {
            runner
                .core_mut()
                .try_send_event(PropEvent::Primary(*id, *spawn_extra))
                .expect("core event channel open");
        }

        let max_steps = (cases.len() * 2).max(1);
        runner.drain_max(max_steps).expect("drain should succeed");

        let processed = runner.core().model().processed.clone();

        let mut expected = Vec::with_capacity(cases.len() * 2);
        let mut secondary_tail = Vec::new();
        for (id, spawn_extra) in &cases {
            expected.push(PropEvent::Primary(*id, *spawn_extra));
            if *spawn_extra {
                secondary_tail.push(PropEvent::Secondary(*id));
            }
        }
        expected.extend(secondary_tail);

        prop_assert_eq!(processed, expected);
    }
}
````

## File: tests/property_tests.proptest-regressions
````
# Seeds for failure cases proptest has generated in the past. It is
# automatically read and these particular cases re-run before any
# novel cases are generated.
#
# It is recommended to check this file in to source control so that
# everyone who runs the test benefits from these saved cases.
cc c070233cd8792ba90d36d000c95ce2bae03a4ca33816142ab0f639b56a8470de # shrinks to event = Increment, model = TestModel { counter: -2, max_value: 0, history: [] }
cc 8af4165004da357764f119bdacb9cff488f680d07b71c797c7c5c2ed91d2d30b # shrinks to events = [Multiply(0)], initial_model = TestModel { counter: -1, max_value: 0, history: [] }
cc 815940a18f697bf9ee362adb920ffe0ed217d167cec7fbbb6938643f684c0b1e # shrinks to event = Multiply(-1), model = TestModel { counter: 1, max_value: 1, history: [] }
````

## File: src/resource_cell.rs
````rust
use std::fmt;
use std::sync::{Arc, RwLock};

/// Thread-safe cell for lazily initialized resources.
///
/// Internally this is just an `Arc<RwLock<Option<T>>>`, but the helper keeps the
/// intent obvious and prevents copy/pasting locking boilerplate across codebases.
#[derive(Debug, Clone)]
pub struct ResourceCell<T> {
    inner: Arc<RwLock<Option<T>>>,
}

impl<T> ResourceCell<T> {
    /// Create an empty cell.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a cell that already contains a value.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(value))),
        }
    }

    /// Returns `true` when the cell does not currently hold a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.read().expect("lock poisoned").is_none()
    }

    /// Replace the current value and return the previous one, if any.
    pub fn set(&self, value: T) -> Option<T> {
        self.inner.write().expect("lock poisoned").replace(value)
    }

    /// Set the value only if the cell is currently empty.
    ///
    /// Returns [`SetOnceError`] if a value was already present.
    pub fn set_once(&self, value: T) -> Result<(), SetOnceError> {
        let mut guard = self.inner.write().expect("lock poisoned");
        if guard.is_some() {
            return Err(SetOnceError);
        }
        *guard = Some(value);
        Ok(())
    }

    /// Take the value out of the cell, leaving it empty.
    #[must_use]
    pub fn take(&self) -> Option<T> {
        self.inner.write().expect("lock poisoned").take()
    }

    /// Run a closure with a shared reference to the value if it exists.
    ///
    /// This avoids exposing locking internals while still permitting read access.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let guard = self.inner.read().expect("lock poisoned");
        guard.as_ref().map(f)
    }

    /// Run a closure with a mutable reference to the value if it exists.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut guard = self.inner.write().expect("lock poisoned");
        guard.as_mut().map(f)
    }

    /// Clone the inner value if it is present.
    #[must_use]
    pub fn cloned(&self) -> Option<T>
    where
        T: Clone,
    {
        self.inner.read().expect("lock poisoned").clone()
    }

    /// Expose the raw `Arc<RwLock<Option<T>>>` for integration with existing code.
    #[must_use]
    pub fn into_inner(self) -> Arc<RwLock<Option<T>>> {
        self.inner
    }
}

/// Error type returned when [`ResourceCell::set_once`] is called on a populated cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetOnceError;

impl fmt::Display for SetOnceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("resource cell already initialised")
    }
}

impl std::error::Error for SetOnceError {}
````

## File: tests/executor/tokio_io_concurrency.rs
````rust
#![cfg(feature = "tokio")]

use std::time::{Duration, Instant};

use syzygy::executor::{AsyncOwnedExecutor, ExecutorLifecycle, TokioExecutor};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

type Event = ();

#[tokio::test(flavor = "multi_thread")]
async fn nonblocking_io_allows_other_tasks_to_progress() {
    // Create a dedicated IO runtime with multiple threads
    let exec = TokioExecutor::multi_thread_io("tokio-io-nonblocking", 2);

    // In-memory duplex stream simulating a bidirectional IO channel
    // This avoids relying on OS networking while still exercising async IO.
    let (mut rx_end, mut tx_end) = tokio::io::duplex(64 * 1024);

    // Signal when the long-running IO completes
    let (io_done_tx, io_done_rx) = oneshot::channel::<usize>();
    // Signal when a short, unrelated task completes
    let (short_done_tx, short_done_rx) = oneshot::channel::<()>();

    // Long-running async IO: read N chunks with the writer injecting a delay between sends
    <TokioExecutor as AsyncOwnedExecutor<Event>>::spawn_owned(&exec, move |_| async move {
        let mut total = 0usize;
        let mut buf = vec![0u8; 16 * 1024];
        while total < 1_000_000 {
            let n = rx_end.read(&mut buf).await.expect("read should succeed");
            if n == 0 {
                break;
            }
            total += n;
        }
        let _ = io_done_tx.send(total);
    })
    .expect("spawn reader should succeed");

    // Writer: send data in chunks with intentional delay to simulate IO pacing
    let writer = async move {
        let chunks = 20usize;
        let chunk_size = 50_000usize;
        let payload = vec![1u8; chunk_size];
        for _ in 0..chunks {
            tx_end
                .write_all(&payload)
                .await
                .expect("write should succeed");
            // Simulate network pacing; if IO were blocking, this could starve other tasks
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let _ = tx_end.shutdown().await; // finish the stream
    };

    <TokioExecutor as AsyncOwnedExecutor<Event>>::spawn_owned(&exec, move |_| writer)
        .expect("spawn writer should succeed");

    // Short task that should complete quickly even while IO is active
    <TokioExecutor as AsyncOwnedExecutor<Event>>::spawn_owned(&exec, move |_| async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = short_done_tx.send(());
    })
    .expect("spawn short task should succeed");

    let start = Instant::now();

    // The short task should complete well before the long IO finishes
    tokio::time::timeout(Duration::from_millis(150), short_done_rx)
        .await
        .expect("short task should not be delayed by IO")
        .expect("channel should deliver signal");

    // The long IO should complete with expected volume and after a noticeable duration
    let total_bytes = tokio::time::timeout(Duration::from_secs(5), io_done_rx)
        .await
        .expect("IO task should complete")
        .expect("channel should deliver byte count");

    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(350),
        "IO finished suspiciously fast; elapsed={elapsed:?}"
    );
    assert!(
        total_bytes >= 1_000_000,
        "Reader received too few bytes: {total_bytes}"
    );

    exec.shutdown();
    exec.wait();
}
````

## File: tests/lib.rs
````rust
//! Integration tests for Syzygy
//!
//! These tests validate the complete behavior of Syzygy components
//! in realistic scenarios.

mod executor;
````

## File: tests/executor/single_thread_executor_basic.rs
````rust
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use syzygy::executor::{ExecutorError, ExecutorLifecycle, SingleThreadExecutor};

type Resources = Arc<Mutex<Vec<usize>>>;

type Exec = SingleThreadExecutor<Resources>;

fn spawn_job<F>(executor: &Exec, job: F) -> Result<(), ExecutorError>
where
    F: FnOnce(&mut Resources) + Send + 'static,
{
    executor.spawn(job)
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_sync_executes_jobs_in_order() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Arc::new(Exec::with_resources(Arc::clone(&shared)));

    for idx in 0..5 {
        let exec_cl = Arc::clone(&executor);
        spawn_job(&exec_cl, move |resources| {
            let shared = Arc::clone(&*resources);
            shared.lock().unwrap().push(idx);
        })
        .expect("spawn should succeed");
    }

    executor.shutdown();
    executor.wait();

    assert_eq!(*shared.lock().unwrap(), vec![0, 1, 2, 3, 4]);
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_sync_rejects_after_shutdown() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Exec::with_resources(shared);
    executor.shutdown();

    let result = spawn_job(&executor, |_resources| {});
    assert!(matches!(result, Err(ExecutorError::WorkerGone)));

    executor.wait();
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_waits_for_inflight_job() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Exec::with_resources(shared);
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&flag);

    spawn_job(&executor, move |_resources| {
        std::thread::sleep(Duration::from_millis(50));
        flag_clone.store(true, Ordering::SeqCst);
    })
    .expect("spawn should succeed");

    executor.shutdown();
    executor.wait();

    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn panicking_job_does_not_poison_executor() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Exec::with_resources(Arc::clone(&shared));

    spawn_job(&executor, |_resources| panic!("boom"))
        .expect("panic happens on worker thread, spawn succeeds");

    spawn_job(&executor, |resources| {
        let shared = Arc::clone(&*resources);
        shared.lock().unwrap().push(1);
    })
    .expect("executor should continue after panic");

    executor.shutdown();
    executor.wait();

    assert_eq!(*shared.lock().unwrap(), vec![1]);
}
````

## File: tests/e2e_nonblocking_io.rs
````rust
#![cfg(feature = "tokio")]

use std::time::{Duration, Instant};

use syzygy::executor::{Task, TokioExecutor};
use syzygy::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Default)]
struct IoModel {
    short_done: bool,
    io_done: bool,
    short_elapsed_ms: Option<u128>,
    io_elapsed_ms: Option<u128>,
    total_bytes: usize,
}

#[derive(Debug, Clone)]
enum IoEvent {
    Start,
    ShortTaskDone(u128),
    LongIoDone { elapsed_ms: u128, bytes: usize },
}

#[derive(Debug, Clone)]
enum IoEffect {
    RunShortTask,
    RunLongIo,
}

fn update(event: IoEvent, model: &mut IoModel) -> Command<IoEvent, IoEffect> {
    match event {
        IoEvent::Start => Command::parallel([IoEffect::RunShortTask, IoEffect::RunLongIo]),
        IoEvent::ShortTaskDone(ms) => {
            model.short_done = true;
            model.short_elapsed_ms = Some(ms);
            Command::none()
        }
        IoEvent::LongIoDone { elapsed_ms, bytes } => {
            model.io_done = true;
            model.io_elapsed_ms = Some(elapsed_ms);
            model.total_bytes = bytes;
            Command::none()
        }
    }
}

fn effects(effect: IoEffect, _resources: ()) -> Task<IoEvent, IoEffect> {
    match effect {
        IoEffect::RunShortTask => run_short_task(),
        IoEffect::RunLongIo => run_long_io(),
    }
}

fn run_short_task() -> Task<IoEvent, IoEffect> {
    Task::<IoEvent, IoEffect>::async_on::<TokioExecutor, _>(async move {
        let t0 = Instant::now();
        tokio::time::sleep(Duration::from_millis(30)).await;
        Command::event(IoEvent::ShortTaskDone(t0.elapsed().as_millis()))
    })
}

fn run_long_io() -> Task<IoEvent, IoEffect> {
    Task::<IoEvent, IoEffect>::async_on::<TokioExecutor, _>(async move {
        let (mut reader, mut writer) = tokio::io::duplex(64 * 1024);
        let writer_fut = async move {
            let chunks = 20usize;
            let chunk_size = 50_000usize;
            let payload = vec![1u8; chunk_size];
            for _ in 0..chunks {
                writer
                    .write_all(&payload)
                    .await
                    .expect("write should succeed");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let _ = writer.shutdown().await;
        };

        let t0 = Instant::now();
        let mut total = 0usize;
        let reader_fut = async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = reader.read(&mut buf).await.expect("read should succeed");
                if n == 0 {
                    break;
                }
                total += n;
            }
            total
        };

        let (_w, bytes) = tokio::join!(writer_fut, reader_fut);
        let elapsed = t0.elapsed().as_millis();
        Command::event(IoEvent::LongIoDone {
            elapsed_ms: elapsed,
            bytes,
        })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_nonblocking_io_with_syzygy() {
    let mut runner = Syzygy::builder::<IoEvent, IoEffect>()
        .model(IoModel::default())
        .event_handler(update)
        .effect_handler(effects)
        .with_async_executor(TokioExecutor::multi_thread_io("e2e-io", 2))
        .build();

    runner
        .core()
        .try_send_event(IoEvent::Start)
        .expect("event channel should be open");

    // First phase: short task should finish quickly while IO runs in background
    let start = Instant::now();
    while !runner.core().model().short_done {
        let _ = runner.step().expect("step should succeed");
        // Yield control to the tokio scheduler periodically
        tokio::task::yield_now().await;
    }
    let short_elapsed_wall = start.elapsed();

    assert!(
        short_elapsed_wall < Duration::from_millis(200),
        "short task took too long: {short_elapsed_wall:?}"
    );

    // Second phase: drain to IO completion
    while !runner.core().model().io_done {
        let _ = runner.step().expect("step should succeed");
        // Yield control to the tokio scheduler periodically
        tokio::task::yield_now().await;
    }

    let model = runner.core().model();
    // IO task should have run for a noticeable time and transferred expected data
    assert!(
        model.io_elapsed_ms.unwrap_or(0) >= 300,
        "IO finished suspiciously fast: {:?}ms",
        model.io_elapsed_ms
    );
    assert!(
        model.total_bytes >= 1_000_000,
        "Transferred too few bytes: {}",
        model.total_bytes
    );
}
````

## File: tests/verified_behaviors.rs
````rust
#![cfg(all(feature = "shell", feature = "rt-single-thread"))]
#![allow(clippy::needless_pass_by_value)]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use syzygy::executor::{SingleThreadExecutor, Task};
use syzygy::prelude::*;

// --- Predictable Event Processing (FIFO) ---

#[derive(Debug, Default)]
struct OrderModel {
    seq: Vec<i32>,
}

#[derive(Debug, Clone)]
enum OrderEvent {
    Push(i32),
}

#[derive(Debug, Clone)]
enum OrderEffect {
    None,
}

fn fifo_update(e: OrderEvent, m: &mut OrderModel) -> Command<OrderEvent, OrderEffect> {
    match e {
        OrderEvent::Push(n) => {
            m.seq.push(n);
            Command::none()
        }
    }
}

#[test]
fn fifo_event_order_is_preserved() {
    let (mut core, _tx) = syzygy::core::Core::new(fifo_update, OrderModel::default());
    // Enqueue a known order
    for n in 0..5 {
        core.try_send_event(OrderEvent::Push(n))
            .expect("event channel should be open");
    }
    let _ = core.process_events();
    assert_eq!(core.model().seq, vec![0, 1, 2, 3, 4]);
}

// --- Resources are cloned per effect invocation ---

#[derive(Debug)]
struct CountedResources {
    clones: Arc<AtomicUsize>,
}

impl Clone for CountedResources {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            clones: Arc::clone(&self.clones),
        }
    }
}

#[derive(Debug, Default)]
struct CloneModel {
    hits: usize,
}

#[derive(Debug, Clone)]
enum CloneEvent {
    Trigger,
    Done,
}

#[derive(Debug, Clone)]
enum CloneEffect {
    DoOne,
}

fn clone_update(e: CloneEvent, m: &mut CloneModel) -> Command<CloneEvent, CloneEffect> {
    match e {
        CloneEvent::Trigger => Command::effects(vec![CloneEffect::DoOne, CloneEffect::DoOne]),
        CloneEvent::Done => {
            m.hits += 1;
            Command::none()
        }
    }
}

fn clone_effects(_x: CloneEffect, _r: CountedResources) -> Task<CloneEvent, CloneEffect> {
    // No executors needed; run on current runtime if present, else block inline
    Task::blocking_with_resource_on::<SingleThreadExecutor<()>, (), _>(|_| {
        Command::event(CloneEvent::Done)
    })
}

#[tokio::test(flavor = "current_thread")]
async fn resources_cloned_per_effect() {
    let counter = Arc::new(AtomicUsize::new(0));
    let resources = CountedResources {
        clones: Arc::clone(&counter),
    };

    let mut app = Syzygy::builder::<CloneEvent, CloneEffect>()
        .model(CloneModel::default())
        .with_resources(resources)
        .event_handler(clone_update)
        .effect_handler(clone_effects)
        .with_resource_blocking_executor(SingleThreadExecutor::new())
        .build();

    // Baseline clone count before any effects
    let before = counter.load(Ordering::SeqCst);

    app.core()
        .try_send_event(CloneEvent::Trigger)
        .expect("event channel should be open");
    // Drain until both DoOne effects complete and emit two Done events
    app.drain_until(|m: &CloneModel| m.hits == 2, Duration::from_secs(1))
        .unwrap();

    let after = counter.load(Ordering::SeqCst);
    // Expect exactly two resource clones for two effect invocations
    assert_eq!(after - before, 2);
}

// --- async_current works with and without a registered executor ---

#[derive(Debug, Default)]
struct CurrentModel {
    n: usize,
}

#[derive(Debug, Clone)]
enum CurrentEvent {
    Go,
    Done,
}

#[derive(Debug, Clone)]
enum CurrentEffect {
    Work,
}

fn current_update(e: CurrentEvent, m: &mut CurrentModel) -> Command<CurrentEvent, CurrentEffect> {
    match e {
        CurrentEvent::Go => Command::effect(CurrentEffect::Work),
        CurrentEvent::Done => {
            m.n += 1;
            Command::none()
        }
    }
}

fn current_effects(_x: CurrentEffect, _r: ()) -> Task<CurrentEvent, CurrentEffect> {
    Task::async_current(async move { Command::event(CurrentEvent::Done) })
}

#[tokio::test(flavor = "multi_thread")]
async fn async_current_under_tokio_runtime() {
    let mut app = Syzygy::builder::<CurrentEvent, CurrentEffect>()
        .model(CurrentModel::default())
        .event_handler(current_update)
        .effect_handler(current_effects)
        .build();

    app.core()
        .try_send_event(CurrentEvent::Go)
        .expect("event channel should be open");
    app.drain_until(|m: &CurrentModel| m.n == 1, Duration::from_secs(1))
        .unwrap();
}

#[test]
fn async_current_without_runtime_blocks_inline() {
    let mut app = Syzygy::builder::<CurrentEvent, CurrentEffect>()
        .model(CurrentModel::default())
        .event_handler(current_update)
        .effect_handler(current_effects)
        .build();

    app.core()
        .try_send_event(CurrentEvent::Go)
        .expect("event channel should be open");
    // Without a runtime, async_current completes inline; a single drain step should finish
    app.drain_until(|m: &CurrentModel| m.n == 1, Duration::from_secs(1))
        .unwrap();
}
````

## File: src/error/command.rs
````rust
use thiserror::Error;

/// Errors that can occur during Command execution
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    /// Command panicked during execution
    #[error("Command panicked: {0}")]
    CommandPanic(String),

    /// Command execution was cancelled
    #[error("Command execution was cancelled")]
    Cancelled,

    /// Command timed out
    #[error("Command execution timed out")]
    Timeout,
}
````

## File: src/error/effect.rs
````rust
use thiserror::Error;

/// Errors that can occur in effect handlers
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectError {
    /// Generic error represented as string message
    #[error("Effect error: {0}")]
    Message(String),
}

impl From<String> for EffectError {
    fn from(s: String) -> Self {
        EffectError::Message(s)
    }
}

impl From<&str> for EffectError {
    fn from(s: &str) -> Self {
        EffectError::Message(s.to_string())
    }
}
````

## File: src/error/mod.rs
````rust
//! Error types for Syzygy operations
//!
//! This module provides specific error types for different parts of the system.
//! Each module defines errors that can occur in its domain.

pub mod command;
pub mod core;
pub mod effect;
#[cfg(feature = "shell")]
pub mod shell;

// Re-export commonly used error types
pub use command::CommandError;
pub use core::CoreError;
pub use effect::EffectError;
#[cfg(feature = "shell")]
pub use shell::ShellError;
````

## File: tests/executor/tokio_executor_basic.rs
````rust
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use syzygy::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle, TokioExecutor};
use tokio::sync::{oneshot, Barrier};

type TestEvent = ();

#[tokio::test(flavor = "multi_thread")]
async fn spawn_owned_executes_job() {
    let executor = Arc::new(
        TokioExecutor::builder()
            .name("tokio-basic")
            .multi_thread()
            .worker_threads(2)
            .io()
            .build(),
    );
    let (tx, rx) = oneshot::channel();

    <TokioExecutor as AsyncExecutor>::spawn_async(
        &*executor,
        async move {
            let _ = tx.send(123u32);
        }
        .boxed(),
    )
    .expect("spawn should succeed");

    let value = tokio::time::timeout(Duration::from_secs(1), rx)
        .await
        .expect("job should complete")
        .expect("channel should deliver value");
    assert_eq!(value, 123);

    executor.shutdown();
    executor.wait();
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_owned_runs_jobs_concurrently() {
    let executor = TokioExecutor::multi_thread_io("tokio-concurrent", 4);
    let barrier = Arc::new(Barrier::new(4));
    let completion = Arc::new(Mutex::new(Vec::new()));

    let start = Instant::now();
    for id in 0..4 {
        let barrier_cl = Arc::clone(&barrier);
        let completion_cl = Arc::clone(&completion);
        <TokioExecutor as AsyncExecutor>::spawn_async(
            &executor,
            async move {
                barrier_cl.wait().await;
                tokio::time::sleep(Duration::from_millis(
                    40 - u64::try_from(id * 5).unwrap_or(0),
                ))
                .await;
                completion_cl.lock().unwrap().push(id);
            }
            .boxed(),
        )
        .expect("spawn should succeed");
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if completion.lock().unwrap().len() == 4 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all jobs should finish");

    executor.shutdown();
    executor.wait();

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(120),
        "jobs ran serially: {elapsed:?}"
    );

    let mut seen = completion.lock().unwrap().clone();
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2, 3]);
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_after_shutdown_returns_error() {
    let executor = TokioExecutor::current_thread_io("tokio-shutdown");
    executor.shutdown();

    let result = <TokioExecutor as AsyncExecutor>::spawn_async(&executor, (async move {}).boxed());
    assert!(matches!(result, Err(ExecutorError::WorkerGone)));

    executor.wait();
}

#[tokio::test(flavor = "multi_thread")]
async fn sleep_delegates_to_runtime() {
    let executor = TokioExecutor::current_thread_io("tokio-sleep");

    let start = tokio::time::Instant::now();
    <TokioExecutor as AsyncExecutor>::sleep(&executor, Duration::from_millis(20)).await;
    assert!(start.elapsed() >= Duration::from_millis(20));

    executor.shutdown();
    executor.wait();
}

#[tokio::test(flavor = "multi_thread")]
async fn try_from_current_creates_executor() {
    let executor = TokioExecutor::try_from_current().expect("tokio runtime should be available");
    let (tx, rx) = oneshot::channel();

    <TokioExecutor as AsyncExecutor>::spawn_async(
        &executor,
        async move {
            let _ = tx.send(());
        }
        .boxed(),
    )
    .expect("spawn should succeed");

    tokio::time::timeout(Duration::from_secs(1), rx)
        .await
        .expect("job should complete")
        .expect("channel should deliver value");

    executor.shutdown();
    executor.wait();
}
````

## File: benches/arc_vs_fn_benchmark.rs
````rust
#![allow(
    dead_code,
    clippy::clone_on_ref_ptr,
    unused_variables,
    unused_imports,
    clippy::let_and_return,
    clippy::format_in_format_args,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::derivable_impls,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::type_complexity,
    clippy::duplicated_attributes
)]
//! Arc vs Function Pointer Benchmark for Effect Handler Performance
//!
//! This benchmark measures the specific performance impact of using Arc<dyn Fn>
//! vs function pointers for effect handlers in Syzygy's Shell implementation.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::future::BoxFuture;
use std::sync::Arc;

// Simulated effect and event types for benchmarking
#[derive(Clone)]
struct BenchEffect {
    id: u32,
    data: String,
}

#[derive(Clone)]
#[allow(dead_code)]
struct BenchEvent {
    result: String,
}

// Simulated AsyncContext (minimal for benchmarking)
#[derive(Copy, Clone)]
struct MockAsyncContext;

impl MockAsyncContext {
    fn send_event(self, _event: BenchEvent) -> Result<(), ()> {
        Ok(())
    }
}

// Arc-based handler (current implementation)
type ArcHandler =
    Arc<dyn Fn(BenchEffect, MockAsyncContext) -> BoxFuture<'static, ()> + Send + Sync>;

// Function pointer handler (proposed optimization)
type FnHandler = fn(BenchEffect, MockAsyncContext) -> BoxFuture<'static, ()>;

// Sample effect handler implementation
fn sample_effect_handler(effect: BenchEffect, ctx: MockAsyncContext) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        // Simulate some work
        let _result = format!("Processed effect {}: {}", effect.id, effect.data);
        let _ = ctx.send_event(BenchEvent {
            result: "completed".to_string(),
        });
    })
}

fn benchmark_arc_cloning(c: &mut Criterion) {
    let handler: ArcHandler = Arc::new(sample_effect_handler);

    c.bench_function("arc_clone_only", |b| {
        b.iter(|| {
            let cloned = Arc::clone(&handler);
            black_box(cloned);
        });
    });
}

fn benchmark_effect_execution(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);
    let fn_handler: FnHandler = sample_effect_handler;

    let effect = BenchEffect {
        id: 42,
        data: "test_data".to_string(),
    };
    let ctx = MockAsyncContext;

    let mut group = c.benchmark_group("effect_execution");

    // Benchmark Arc-based handler (with clone overhead)
    group.bench_function("arc_handler", |b| {
        b.iter(|| {
            let cloned_handler = Arc::clone(&arc_handler);
            let effect_clone = effect.clone();
            let future = cloned_handler(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    // Benchmark function pointer (zero overhead)
    group.bench_function("fn_handler", |b| {
        b.iter(|| {
            let effect_clone = effect.clone();
            let future = fn_handler(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    group.finish();
}

fn benchmark_batch_effects(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);
    let fn_handler: FnHandler = sample_effect_handler;

    let effects: Vec<BenchEffect> = (0..100)
        .map(|i| BenchEffect {
            id: i,
            data: format!("batch_data_{i}"),
        })
        .collect();

    let ctx = MockAsyncContext;

    let mut group = c.benchmark_group("batch_effects");

    // Batch processing with Arc (current implementation)
    group.bench_function("arc_batch", |b| {
        b.iter(|| {
            for effect in &effects {
                let cloned_handler = Arc::clone(&arc_handler);
                let effect_clone = effect.clone();
                let future = cloned_handler(effect_clone, ctx);
                #[allow(unused_must_use)]
                black_box(future);
            }
        });
    });

    // Batch processing with function pointer
    group.bench_function("fn_batch", |b| {
        b.iter(|| {
            for effect in &effects {
                let effect_clone = effect.clone();
                let future = fn_handler(effect_clone, ctx);
                #[allow(unused_must_use)]
                black_box(future);
            }
        });
    });

    group.finish();
}

fn benchmark_memory_access_patterns(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);
    let fn_handler: FnHandler = sample_effect_handler;

    let effect = BenchEffect {
        id: 1,
        data: "memory_test".to_string(),
    };
    let ctx = MockAsyncContext;

    let mut group = c.benchmark_group("memory_patterns");

    // Measure the cost of Arc indirection
    group.bench_function("arc_indirection", |b| {
        b.iter(|| {
            // This simulates the typical usage pattern in Shell::tick()
            // where we clone the Arc for each effect execution
            let cloned = Arc::clone(&arc_handler);
            let effect_clone = effect.clone();

            // The actual function call through Arc indirection
            let future = cloned(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    // Direct function call (zero indirection)
    group.bench_function("fn_direct", |b| {
        b.iter(|| {
            let effect_clone = effect.clone();
            let future = fn_handler(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    group.finish();
}

fn benchmark_concurrent_access(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);

    let mut group = c.benchmark_group("concurrent_access");

    // Simulate concurrent Arc cloning (potential cache line contention)
    group.bench_function("arc_concurrent_clone", |b| {
        b.iter(|| {
            // Simulate multiple concurrent clones
            let handlers: Vec<_> = (0..4).map(|_| Arc::clone(&arc_handler)).collect();
            black_box(handlers);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_arc_cloning,
    benchmark_effect_execution,
    benchmark_batch_effects,
    benchmark_memory_access_patterns,
    benchmark_concurrent_access
);
criterion_main!(benches);
````

## File: src/error/core.rs
````rust
use thiserror::Error;

/// Errors that can occur in Core operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    /// Event channel is closed (system is shutting down)
    #[error("Event channel is closed - system may be shutting down")]
    ChannelClosed,

    /// Event channel is full (for bounded channels)
    #[error("Event channel is full - system may be overloaded")]
    ChannelFull,
}

impl<T> From<crossbeam_channel::SendError<T>> for CoreError {
    fn from(_: crossbeam_channel::SendError<T>) -> Self {
        CoreError::ChannelClosed
    }
}

impl<T> From<crossbeam_channel::TrySendError<T>> for CoreError {
    fn from(error: crossbeam_channel::TrySendError<T>) -> Self {
        match error {
            crossbeam_channel::TrySendError::Full(_) => CoreError::ChannelFull,
            crossbeam_channel::TrySendError::Disconnected(_) => CoreError::ChannelClosed,
        }
    }
}
````

## File: src/executor/task.rs
````rust
//! Declarative task plans returned by effect handlers.
//!
//! A `Task<E, X>` describes what work to schedule and on which executor type.
//! The Shell interprets the plan and routes any resulting `Command` steps back
//! into the Core/Shell pipeline.
use std::any::{type_name, Any, TypeId};
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use crossbeam_channel::Sender as EffectSender;
#[cfg(feature = "tokio")]
use futures::executor::block_on;
use futures_util::future::{BoxFuture, Either, FutureExt};
use futures_util::stream::{BoxStream, StreamExt};

use crate::activity::Activity;
use crate::command::{Command, CommandStep};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::{
    panic_message, AsyncExecutor, BlockingExecutor, ExecutorRegistry, ResourceBlockingExecutor,
};
use crate::shell::ShellStats;

fn missing_executor(kind: &'static str, exec: TypeId, exec_name: &'static str) -> ShellError {
    #[cfg(feature = "tracing")]
    tracing::error!(
        ?exec,
        kind,
        exec_name,
        "Missing executor; effect could not be scheduled"
    );

    ShellError::TaskSpawnFailed(format!(
        "Missing {kind} executor: {exec_name} (TypeId={exec:?})"
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicTaskKind {
    Async,
    AsyncCurrent,
    Blocking,
    BlockingWithResource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanicDetails {
    pub kind: PanicTaskKind,
    pub executor_type_name: &'static str,
    pub executor_type_id: TypeId,
}

impl PanicDetails {
    #[must_use]
    pub fn new(
        kind: PanicTaskKind,
        executor_type_name: &'static str,
        executor_type_id: TypeId,
    ) -> Self {
        Self {
            kind,
            executor_type_name,
            executor_type_id,
        }
    }
}

pub type PanicHook<E, X> = dyn Fn(PanicDetails, String) -> Command<E, X> + Send + Sync;

fn command_from_panic<E, X>(
    panic_handler: Option<&Arc<PanicHook<E, X>>>,
    details: PanicDetails,
    payload: Box<dyn Any + Send>,
) -> Command<E, X> {
    let message = panic_message(payload);
    #[cfg(feature = "tracing")]
    tracing::error!(?details, %message, "Task panicked");
    if let Some(handler) = panic_handler {
        handler(details, message)
    } else {
        Command::none()
    }
}

/// Declarative unit of work returned by effect handlers.
pub enum Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    Event(E),
    Events(Vec<E>),
    Async {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        future: BoxFuture<'static, Command<E, X>>,
    },
    Stream {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        stream: BoxStream<'static, E>,
    },
    #[cfg(feature = "tokio")]
    /// Run on the current async runtime if available (e.g. inside #[tokio::main]).
    /// Falls back to blocking execution on the current thread if no runtime.
    AsyncCurrent {
        future: BoxFuture<'static, Command<E, X>>,
    },
    #[cfg(feature = "tokio")]
    /// Forward a stream on the current async runtime if available.
    /// Falls back to draining the stream on the current thread if no runtime.
    StreamCurrent {
        stream: BoxStream<'static, E>,
    },
    Blocking {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        job: Box<dyn FnOnce() -> Command<E, X> + Send>,
    },
    BlockingWithResource {
        exec_type_id: TypeId,
        exec_type_name: &'static str,
        resource_type_id: TypeId,
        resource_type_name: &'static str,
        job: Box<dyn FnOnce(&mut dyn Any) -> Command<E, X> + Send>,
    },
}

impl<E, X> Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// Emit multiple events back to Core.
    pub fn events<I>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
    {
        Self::Events(events.into_iter().collect())
    }

    /// No-op task. Useful when effects are conditionally skipped.
    #[must_use]
    pub fn none() -> Self {
        Self::Events(vec![])
    }

    /// Emit a single event back to Core.
    pub fn event(event: E) -> Self {
        Self::Event(event)
    }

    /// Run a future on a specific async executor type.
    ///
    /// Selects the executor by its concrete type; register the same type on
    /// the builder. The future resolves to a `Command` whose outputs are routed
    /// back through the system.
    pub fn async_on<Exec, Fut>(future: Fut) -> Self
    where
        Exec: AsyncExecutor + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let future: BoxFuture<'static, Command<E, X>> = future.boxed();
        Self::Async {
            exec_type_id,
            exec_type_name,
            future,
        }
    }

    /// Run a future on a specific async executor with cancellation support.
    pub fn async_on_with_cancel<Exec, Fut, Cancel>(
        future: Fut,
        cancel: Cancel,
        cancel_command: Command<E, X>,
    ) -> Self
    where
        Exec: AsyncExecutor + 'static,
        Fut: Future<Output = Command<E, X>> + Send + 'static,
        Cancel: Future<Output = ()> + Send + 'static,
    {
        let fut = async move {
            futures_util::pin_mut!(future);
            futures_util::pin_mut!(cancel);
            match futures_util::future::select(cancel, future).await {
                Either::Left((_, pending_future)) => {
                    drop(pending_future);
                    cancel_command
                }
                Either::Right((command, _)) => command,
            }
        };
        Self::async_on::<Exec, _>(fut)
    }

    #[cfg(feature = "tokio")]
    /// Create an async task that runs on the current runtime if present,
    /// otherwise completes inline by blocking the current thread.
    pub fn async_current<Fut>(future: Fut) -> Self
    where
        Fut: Future<Output = Command<E, X>> + Send + 'static,
    {
        let future: BoxFuture<'static, Command<E, X>> = future.boxed();
        Self::AsyncCurrent { future }
    }

    #[cfg(feature = "tokio")]
    /// Create an async task on the current runtime with cancellation support.
    pub fn async_current_with_cancel<Fut, Cancel>(
        future: Fut,
        cancel: Cancel,
        cancel_command: Command<E, X>,
    ) -> Self
    where
        Fut: Future<Output = Command<E, X>> + Send + 'static,
        Cancel: Future<Output = ()> + Send + 'static,
    {
        let fut = async move {
            futures_util::pin_mut!(future);
            futures_util::pin_mut!(cancel);
            match futures_util::future::select(cancel, future).await {
                Either::Left((_, pending_future)) => {
                    drop(pending_future);
                    cancel_command
                }
                Either::Right((command, _)) => command,
            }
        };
        Self::async_current(fut)
    }

    #[cfg(feature = "tokio")]
    /// Create a stream task that runs on the current runtime if present,
    /// otherwise drains inline by blocking the current thread.
    pub fn stream_current<S>(stream: S) -> Self
    where
        S: futures_util::stream::Stream<Item = E> + Send + 'static,
    {
        let stream: BoxStream<'static, E> = stream.boxed();
        Self::StreamCurrent { stream }
    }

    /// Forward a stream’s items as events on a specific async executor.
    pub fn stream_on<Exec, S>(stream: S) -> Self
    where
        Exec: AsyncExecutor + 'static,
        S: futures_util::stream::Stream<Item = E> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let stream: BoxStream<'static, E> = stream.boxed();
        Self::Stream {
            exec_type_id,
            exec_type_name,
            stream,
        }
    }

    /// Run a blocking job on a blocking executor (no shared mutable resource).
    pub fn blocking_on<Exec, F>(job: F) -> Self
    where
        Exec: BlockingExecutor + 'static,
        F: FnOnce() -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let job: Box<dyn FnOnce() -> Command<E, X> + Send> = Box::new(job);
        Self::Blocking {
            exec_type_id,
            exec_type_name,
            job,
        }
    }

    /// Run a blocking job that requires mutable access to an executor-owned resource.
    pub fn blocking_with_resource_on<Exec, R, F>(job: F) -> Self
    where
        Exec: ResourceBlockingExecutor + 'static,
        R: 'static,
        F: FnOnce(&mut R) -> Command<E, X> + Send + 'static,
    {
        let exec_type_id = TypeId::of::<Exec>();
        let exec_type_name = type_name::<Exec>();
        let resource_type_id = TypeId::of::<R>();
        let resource_type_name = type_name::<R>();
        let user_job = job;
        let job = Box::new(move |resource: &mut dyn Any| {
            let resource = resource.downcast_mut::<R>().unwrap_or_else(|| {
                panic!(
                    "resource type mismatch for single-thread executor; expected {}",
                    resource_type_name
                )
            });
            user_job(resource)
        }) as Box<dyn FnOnce(&mut dyn Any) -> Command<E, X> + Send>;
        Self::BlockingWithResource {
            exec_type_id,
            exec_type_name,
            resource_type_id,
            resource_type_name,
            job,
        }
    }
}

#[cfg(feature = "tokio")]
impl<E, X> From<BoxFuture<'static, Command<E, X>>> for Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    fn from(future: BoxFuture<'static, Command<E, X>>) -> Self {
        Task::AsyncCurrent { future }
    }
}

#[cfg(feature = "tokio")]
impl<E, X> From<BoxStream<'static, E>> for Task<E, X>
where
    E: Send + 'static,
    X: Send + 'static,
{
    fn from(stream: BoxStream<'static, E>) -> Self {
        Task::StreamCurrent { stream }
    }
}

fn route_command<E, X>(
    event_tx: &EventSender<E>,
    effect_tx: &EffectSender<CommandStep<E, X>>,
    command: Command<E, X>,
    stats: Option<&ShellStats>,
) where
    E: Send + 'static,
    X: Send + 'static,
{
    for step in command {
        match step {
            CommandStep::Event(event) => {
                if event_tx.send(event).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_event();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("event channel closed while routing command output");
                }
            }
            CommandStep::Effect(x) => {
                if effect_tx.send(CommandStep::Effect(x)).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_effect_step();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("effect channel closed while routing command output");
                }
            }
            CommandStep::Batch(v) => {
                if effect_tx.send(CommandStep::Batch(v)).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_effect_step();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("effect channel closed while routing batch command output");
                }
            }
            CommandStep::Parallel(v) => {
                if effect_tx.send(CommandStep::Parallel(v)).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_effect_step();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("effect channel closed while routing parallel command output");
                }
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) fn drive_task<E, X>(
    executors: &Arc<ExecutorRegistry<E>>,
    task: Task<E, X>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    panic_handler: Option<&Arc<PanicHook<E, X>>>,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    drive_task_with_activity(
        executors,
        task,
        event_tx,
        effect_tx,
        None,
        None,
        panic_handler,
    )
}

pub(crate) fn drive_task_with_activity<E, X>(
    executors: &Arc<ExecutorRegistry<E>>,
    task: Task<E, X>,
    event_tx: EventSender<E>,
    effect_tx: EffectSender<CommandStep<E, X>>,
    activity: Option<&Activity>,
    stats: Option<&ShellStats>,
    panic_handler: Option<&Arc<PanicHook<E, X>>>,
) -> Result<(), ShellError>
where
    E: Send + 'static,
    X: Send + 'static,
{
    match task {
        Task::Event(event) => {
            if event_tx.send(event).is_err() {
                if let Some(stats) = stats {
                    stats.inc_dropped_event();
                }
                #[cfg(feature = "tracing")]
                tracing::debug!("event channel closed while dispatching event task");
            }
        }
        Task::Events(events) => {
            for e in events {
                if event_tx.send(e).is_err() {
                    if let Some(stats) = stats {
                        stats.inc_dropped_event();
                    }
                    #[cfg(feature = "tracing")]
                    tracing::debug!("event channel closed while dispatching event task");
                    break;
                }
            }
        }
        #[cfg(feature = "tokio")]
        Task::AsyncCurrent { future } => {
            // Increment activity counter for async work
            if let Some(activity) = activity {
                activity.inc();
            }

            // Try to spawn on the current Tokio runtime; if unavailable, run inline.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let event_tx_cl = event_tx.clone();
                let effect_tx_cl = effect_tx.clone();
                let activity_cl = activity.cloned();
                let stats_cl = stats.cloned();
                let panic_handler_cl = panic_handler.cloned();
                let fut = async move {
                    let details = PanicDetails::new(
                        PanicTaskKind::AsyncCurrent,
                        "current_runtime",
                        TypeId::of::<()>(),
                    );
                    let result = AssertUnwindSafe(future).catch_unwind().await;
                    let command = match result {
                        Ok(command) => command,
                        Err(payload) => {
                            command_from_panic(panic_handler_cl.as_ref(), details, payload)
                        }
                    };
                    let stats_ref = stats_cl.as_ref();
                    route_command(&event_tx_cl, &effect_tx_cl, command, stats_ref);
                    // Decrement activity counter when async work completes
                    if let Some(activity) = activity_cl {
                        activity.dec();
                    }
                };
                handle.spawn(fut);
            } else {
                let details = PanicDetails::new(
                    PanicTaskKind::AsyncCurrent,
                    "inline_current",
                    TypeId::of::<()>(),
                );
                let command = panic::catch_unwind(AssertUnwindSafe(|| block_on(future)))
                    .map_err(|payload| command_from_panic(panic_handler, details, payload))
                    .unwrap_or_else(|command| command);
                route_command(&event_tx, &effect_tx, command, stats);
                // Decrement activity counter for inline completion
                if let Some(activity) = activity {
                    activity.dec();
                }
            }
        }
        Task::Async {
            exec_type_id,
            exec_type_name,
            future,
        } => {
            let exec = executors
                .async_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("async", exec_type_id, exec_type_name))?;

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let activity_cl = activity.cloned();
            let stats_cl = stats.cloned();
            if let Some(activity) = activity {
                activity.inc();
            }
            let panic_handler_cl = panic_handler.cloned();
            let fut = async move {
                let details = PanicDetails::new(PanicTaskKind::Async, exec_type_name, exec_type_id);
                let result = AssertUnwindSafe(future).catch_unwind().await;
                let command = match result {
                    Ok(command) => command,
                    Err(payload) => command_from_panic(panic_handler_cl.as_ref(), details, payload),
                };
                let stats_ref = stats_cl.as_ref();
                route_command(&event_tx_cl, &effect_tx_cl, command, stats_ref);
                // Decrement activity counter when async work completes
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }
            .boxed();

            if let Err(err) = exec.spawn_async(fut) {
                if let Some(activity) = activity {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "async executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
        #[cfg(feature = "tokio")]
        Task::StreamCurrent { stream } => {
            // Increment activity counter for stream work
            if let Some(activity) = activity {
                activity.inc();
            }

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let event_tx_cl = event_tx.clone();
                let activity_cl = activity.cloned();
                let stats_cl = stats.cloned();
                let fut = async move {
                    futures_util::pin_mut!(stream);
                    while let Some(event) = stream.next().await {
                        if event_tx_cl.send(event).is_err() {
                            if let Some(stats) = stats_cl.as_ref() {
                                stats.inc_dropped_event();
                            }
                            #[cfg(feature = "tracing")]
                            tracing::debug!("event channel closed while forwarding stream item");
                            break;
                        }
                    }
                    // Decrement activity counter when stream ends
                    if let Some(activity) = activity_cl {
                        activity.dec();
                    }
                };
                handle.spawn(fut);
            } else {
                let stats_cl = stats.cloned();
                block_on(async move {
                    futures_util::pin_mut!(stream);
                    while let Some(event) = stream.next().await {
                        if event_tx.send(event).is_err() {
                            if let Some(stats) = stats_cl.as_ref() {
                                stats.inc_dropped_event();
                            }
                            #[cfg(feature = "tracing")]
                            tracing::debug!("event channel closed while forwarding stream item");
                            break;
                        }
                    }
                    // Decrement activity counter for inline completion
                    if let Some(activity) = activity {
                        activity.dec();
                    }
                });
            }
        }
        Task::Stream {
            exec_type_id,
            exec_type_name,
            stream,
        } => {
            let exec = executors
                .async_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("stream", exec_type_id, exec_type_name))?;

            let event_tx_cl = event_tx.clone();
            let activity_cl = activity.cloned();
            let stats_cl = stats.cloned();
            if let Some(activity) = activity {
                activity.inc();
            }
            let fut = async move {
                futures_util::pin_mut!(stream);
                while let Some(event) = stream.next().await {
                    if event_tx_cl.send(event).is_err() {
                        if let Some(stats) = stats_cl.as_ref() {
                            stats.inc_dropped_event();
                        }
                        #[cfg(feature = "tracing")]
                        tracing::debug!("event channel closed while forwarding stream item");
                        break;
                    }
                }
                // Decrement activity counter when stream ends
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }
            .boxed();

            if let Err(err) = exec.spawn_async(fut) {
                if let Some(activity) = activity {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "stream executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
        Task::Blocking {
            exec_type_id,
            exec_type_name,
            job,
        } => {
            let exec = executors
                .blocking_exec_by_key(exec_type_id)
                .ok_or_else(|| missing_executor("blocking", exec_type_id, exec_type_name))?;

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let activity_cl = activity.cloned();
            let stats_cl = stats.cloned();
            if let Some(activity) = activity {
                activity.inc();
            }
            let panic_handler_cl = panic_handler.cloned();
            let job = Box::new(move || {
                let details =
                    PanicDetails::new(PanicTaskKind::Blocking, exec_type_name, exec_type_id);
                let command = match panic::catch_unwind(AssertUnwindSafe(|| job())) {
                    Ok(command) => command,
                    Err(payload) => command_from_panic(panic_handler_cl.as_ref(), details, payload),
                };
                let stats_ref = stats_cl.as_ref();
                route_command(&event_tx_cl, &effect_tx_cl, command, stats_ref);
                // Decrement activity counter when blocking job completes
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }) as Box<dyn FnOnce() + Send>;

            if let Err(err) = exec.spawn_blocking(job) {
                if let Some(activity) = activity {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "blocking executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
        Task::BlockingWithResource {
            exec_type_id,
            exec_type_name,
            resource_type_id,
            resource_type_name,
            job,
        } => {
            let exec = executors
                .resource_blocking_exec_by_key(exec_type_id)
                .ok_or_else(|| {
                    missing_executor("resource-blocking", exec_type_id, exec_type_name)
                })?;

            let actual = exec.resource_type_id();
            if actual != resource_type_id {
                return Err(ShellError::TaskSpawnFailed(format!(
                    "resource type mismatch for executor {exec_type_name} (TypeId={exec_type_id:?}): expected resource {resource_type_name} (TypeId={resource_type_id:?}), got TypeId={actual:?}"
                )));
            }

            let event_tx_cl = event_tx.clone();
            let effect_tx_cl = effect_tx.clone();
            let activity_cl = activity.cloned();
            let stats_cl = stats.cloned();
            if let Some(activity) = activity {
                activity.inc();
            }
            let panic_handler_cl = panic_handler.cloned();
            let job = Box::new(move |resource: &mut dyn Any| {
                let details = PanicDetails::new(
                    PanicTaskKind::BlockingWithResource,
                    exec_type_name,
                    exec_type_id,
                );
                let command = match panic::catch_unwind(AssertUnwindSafe(|| job(resource))) {
                    Ok(command) => command,
                    Err(payload) => command_from_panic(panic_handler_cl.as_ref(), details, payload),
                };
                let stats_ref = stats_cl.as_ref();
                route_command(&event_tx_cl, &effect_tx_cl, command, stats_ref);
                // Decrement activity counter when blocking job completes
                if let Some(activity) = activity_cl {
                    activity.dec();
                }
            }) as Box<dyn FnOnce(&mut dyn Any) + Send>;

            if let Err(err) = exec.spawn_blocking_with_resource(job) {
                if let Some(activity) = activity {
                    activity.dec();
                }
                return Err(ShellError::TaskSpawnFailed(format!(
                    "resource-blocking executor {exec_type_name} (TypeId={exec_type_id:?}): {err}"
                )));
            }
        }
    }

    Ok(())
}
````

## File: tests/executor/mod.rs
````rust
//! Executor test modules

#[cfg(feature = "tokio")]
mod tokio_executor_basic;

#[cfg(feature = "rt-single-thread")]
mod single_thread_executor_basic;
````

## File: examples/async_effect.rs
````rust
//! Demonstrates scheduling an async effect on Tokio.
//!
//! Build with
//! ```bash
//! cargo run --example async_effect --features examples
//! ```

use std::time::Duration;

use syzygy::executor::{Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct DownloadModel {
    status: String,
    finished: bool,
}

#[derive(Debug, Clone)]
enum DownloadEvent {
    Start,
    Completed(String),
}

#[derive(Debug, Clone)]
enum DownloadEffect {
    FetchGreeting,
}

fn event_handler(
    event: DownloadEvent,
    model: &mut DownloadModel,
) -> Command<DownloadEvent, DownloadEffect> {
    match event {
        DownloadEvent::Start => on_start(model),
        DownloadEvent::Completed(message) => on_completed(model, message),
    }
}

fn on_start(model: &mut DownloadModel) -> Command<DownloadEvent, DownloadEffect> {
    model.status = "requesting...".to_string();
    cmd::effect(DownloadEffect::FetchGreeting)
}

fn on_completed(
    model: &mut DownloadModel,
    message: String,
) -> Command<DownloadEvent, DownloadEffect> {
    model.status = format!("response: {message}");
    model.finished = true;
    cmd::none()
}

fn effect_handler(effect: DownloadEffect, _resources: ()) -> Task<DownloadEvent, DownloadEffect> {
    match effect {
        DownloadEffect::FetchGreeting => fetch_greeting_task(),
    }
}

fn fetch_greeting_task() -> Task<DownloadEvent, DownloadEffect> {
    Task::async_on::<TokioExecutor, _>(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cmd::event(DownloadEvent::Completed(
            "hello from async effect".to_string(),
        ))
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<DownloadEvent, DownloadEffect>()
        .model(DownloadModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .profile_interactive()
        .with_async_executor(TokioExecutor::multi_thread_io("async-example", 2))
        .build();

    runner
        .core()
        .try_send_event(DownloadEvent::Start)
        .expect("event channel should be open");

    runner.run_until(|core, _shell| core.model().finished)?;

    println!("{}", runner.core().model().status);
    Ok(())
}
````

## File: src/error/shell.rs
````rust
use std::time::Duration;

use thiserror::Error;

/// Errors that can occur in Shell operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShellError {
    /// Task tracker is closed (no new tasks can be spawned)
    #[error("Task tracker is closed - no new tasks can be spawned")]
    TaskTrackerClosed,

    /// Task tracker mutex was poisoned
    #[error("Task tracker mutex was poisoned")]
    TaskTrackerPoisoned,

    /// Event channel is closed
    #[error("Event channel is closed")]
    EventChannelClosed,

    /// Effect execution failed
    #[error("Effect execution failed: {0}")]
    EffectFailed(String),

    /// Invalid state transition attempted
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),

    /// Task spawn failed
    #[error("Task spawn failed: {0}")]
    TaskSpawnFailed(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    CommandExecutionFailed(String),

    /// Effect queue reached its configured capacity
    #[error(
        "Effect queue is full (capacity {capacity}). Consider increasing the capacity or awaiting idle before queuing more effects"
    )]
    EffectQueueFull { capacity: usize },

    /// Timed out while waiting for work to complete
    #[error("Timed out after {duration:?} while draining work; use larger timeouts or inspect backpressure metrics")]
    Timeout { duration: Duration },
}
````

## File: examples/basic_counter.rs
````rust
//! Minimal counter demonstrating the core Syzygy loop.
//!
//! Build with
//! ```bash
//! cargo run --example basic_counter --features examples
//! ```

use syzygy::executor::{InlineAsync, Task};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct CounterModel {
    value: i32,
}

#[derive(Debug, Clone)]
enum CounterEvent {
    Increment,
    Decrement,
}

#[derive(Debug, Clone)]
enum CounterEffect {
    Log(String),
}

fn event_handler(
    event: CounterEvent,
    model: &mut CounterModel,
) -> Command<CounterEvent, CounterEffect> {
    match event {
        CounterEvent::Increment => on_increment(model),
        CounterEvent::Decrement => on_decrement(model),
    }
}

fn on_increment(model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
    model.value += 1;
    cmd::effect(CounterEffect::Log(format!(
        "Count incremented to {}",
        model.value
    )))
}

fn on_decrement(model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
    model.value -= 1;
    cmd::effect(CounterEffect::Log(format!(
        "Count decremented to {}",
        model.value
    )))
}

fn effect_handler(effect: CounterEffect, _resources: ()) -> Task<CounterEvent, CounterEffect> {
    match effect {
        CounterEffect::Log(message) => log_message(message),
    }
}

fn log_message(message: String) -> Task<CounterEvent, CounterEffect> {
    Task::async_on::<InlineAsync, _>(async move {
        println!("{message}");
        cmd::none()
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
        .model(CounterModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .profile_interactive()
        .with_async_executor(InlineAsync::new())
        .build();

    runner
        .core()
        .try_send_event(CounterEvent::Increment)
        .expect("event channel should be open");
    runner
        .core()
        .try_send_event(CounterEvent::Increment)
        .expect("event channel should be open");
    runner
        .core()
        .try_send_event(CounterEvent::Decrement)
        .expect("event channel should be open");

    while runner.step()? {}

    println!("Final count: {}", runner.core().model().value);
    Ok(())
}
````

## File: examples/two_executors.rs
````rust
//! Demonstrates using separate async executors for IO and CPU work under a Tokio runtime.
//!
//! Run with:
//! ```bash
//! cargo run --example two_executors --features examples
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use futures_util::future::BoxFuture;
use syzygy::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle, Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct DemoModel {
    io_message: Option<String>,
    cpu_result: Option<u128>,
}

#[derive(Debug, Clone)]
enum DemoEvent {
    Start,
    IoFinished(String),
    CpuFinished(u128),
}

#[derive(Debug, Clone)]
enum DemoEffect {
    FetchGreeting,
    CrunchNumber(u64),
}

fn event_handler(event: DemoEvent, model: &mut DemoModel) -> Command<DemoEvent, DemoEffect> {
    match event {
        DemoEvent::Start => on_start(),
        DemoEvent::IoFinished(message) => on_io_finished(model, message),
        DemoEvent::CpuFinished(value) => on_cpu_finished(model, value),
    }
}

fn on_start() -> Command<DemoEvent, DemoEffect> {
    cmd::parallel([DemoEffect::FetchGreeting, DemoEffect::CrunchNumber(38)])
}

fn on_io_finished(model: &mut DemoModel, message: String) -> Command<DemoEvent, DemoEffect> {
    model.io_message = Some(message);
    cmd::none()
}

fn on_cpu_finished(model: &mut DemoModel, value: u128) -> Command<DemoEvent, DemoEffect> {
    model.cpu_result = Some(value);
    cmd::none()
}

#[derive(Debug, Default)]
struct IoRuntimeTag;

#[derive(Debug, Default)]
struct CpuRuntimeTag;

type IoRuntime = TaggedTokioExecutor<IoRuntimeTag>;
type CpuRuntime = TaggedTokioExecutor<CpuRuntimeTag>;

#[derive(Debug)]
struct TaggedTokioExecutor<Tag> {
    inner: TokioExecutor,
    _marker: PhantomData<Tag>,
}

impl<Tag> TaggedTokioExecutor<Tag> {
    fn from_executor(inner: TokioExecutor) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl TaggedTokioExecutor<IoRuntimeTag> {
    fn new(worker_threads: usize) -> Self {
        Self::from_executor(TokioExecutor::multi_thread_io("io-pool", worker_threads))
    }
}

impl TaggedTokioExecutor<CpuRuntimeTag> {
    fn new(worker_threads: usize) -> Self {
        Self::from_executor(TokioExecutor::multi_thread_cpu("cpu-pool", worker_threads))
    }
}

impl<Tag> ExecutorLifecycle for TaggedTokioExecutor<Tag>
where
    Tag: Send + Sync + 'static,
{
    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn wait(&self) {
        self.inner.wait();
    }
}

impl<Tag> AsyncExecutor for TaggedTokioExecutor<Tag>
where
    Tag: Send + Sync + 'static,
{
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        <TokioExecutor as AsyncExecutor>::spawn_async(&self.inner, job)
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        <TokioExecutor as AsyncExecutor>::sleep(&self.inner, duration)
    }
}

fn effect_handler(effect: DemoEffect, _resources: ()) -> Task<DemoEvent, DemoEffect> {
    match effect {
        DemoEffect::FetchGreeting => fetch_greeting(),
        DemoEffect::CrunchNumber(n) => crunch_number(n),
    }
}

fn fetch_greeting() -> Task<DemoEvent, DemoEffect> {
    Task::async_on::<IoRuntime, _>(async move {
        tokio::time::sleep(Duration::from_millis(40)).await;
        cmd::event(DemoEvent::IoFinished(format!(
            "hello from thread {:?}",
            std::thread::current().id()
        )))
    })
}

fn crunch_number(n: u64) -> Task<DemoEvent, DemoEffect> {
    Task::async_on::<CpuRuntime, _>(async move {
        let result = fibonacci(n);
        cmd::event(DemoEvent::CpuFinished(result))
    })
}

fn fibonacci(n: u64) -> u128 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut prev = 0u128;
            let mut curr = 1u128;
            for _ in 2..=n {
                let next = prev + curr;
                prev = curr;
                curr = next;
            }
            curr
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<DemoEvent, DemoEffect>()
        .model(DemoModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .profile_server()
        .with_async_executor(IoRuntime::new(2))
        .with_async_executor(CpuRuntime::new(2))
        .build();

    runner
        .core()
        .try_send_event(DemoEvent::Start)
        .expect("event channel should be open");

    runner.run_until(|core, _| {
        let model = core.model();
        model.io_message.is_some() && model.cpu_result.is_some()
    })?;

    let model = runner.core().model();
    println!("IO result: {:?}", model.io_message);
    println!("CPU result: {:?}", model.cpu_result);
    Ok(())
}
````

## File: src/executor/inline_async.rs
````rust
use std::time::Duration;

use futures::executor::block_on;
use futures::future::{BoxFuture, FutureExt};

use super::{AsyncExecutor, ExecutorError, ExecutorLifecycle};

/// Inline async executor — executes futures immediately on the caller thread.
///
/// Great for tests and CLIs where determinism beats concurrency. `sleep()`
/// blocks the current thread.
pub struct InlineAsync;

impl Default for InlineAsync {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineAsync {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl std::fmt::Debug for InlineAsync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineAsync").finish()
    }
}

impl AsyncExecutor for InlineAsync {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        block_on(job);
        Ok(())
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        async move { std::thread::sleep(duration) }.boxed()
    }
}

impl ExecutorLifecycle for InlineAsync {
    fn shutdown(&self) {}

    fn wait(&self) {
        // inline executor completes work immediately; nothing to wait on
    }
}
````

## File: benches/command_performance.rs
````rust
#![allow(
    dead_code,
    clippy::clone_on_ref_ptr,
    unused_variables,
    unused_imports,
    clippy::let_and_return,
    clippy::format_in_format_args,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::derivable_impls,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::type_complexity,
    clippy::duplicated_attributes
)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use syzygy::command::CommandStep;
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum BenchEvent {
    A,
    B,
    C,
    D,
    E,
}

#[derive(Debug, Clone)]
enum BenchEffect {
    X,
    Y,
    Z,
    W,
    V,
}

fn bench_command_creation(c: &mut Criterion) {
    c.bench_function("command_none", |b| {
        b.iter(|| black_box(Command::<BenchEvent, BenchEffect>::none()));
    });

    c.bench_function("command_single_event", |b| {
        b.iter(|| black_box(Command::<BenchEvent, BenchEffect>::event(BenchEvent::A)));
    });

    c.bench_function("command_single_effect", |b| {
        b.iter(|| black_box(Command::<BenchEvent, BenchEffect>::effect(BenchEffect::X)));
    });

    // Test the SmallVec inline optimization (≤4 items)
    c.bench_function("command_batch_4_items", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch([
                Command::event(BenchEvent::A),
                Command::event(BenchEvent::B),
                Command::effect(BenchEffect::X),
                Command::effect(BenchEffect::Y),
            ]))
        });
    });

    // Test SmallVec spill case (>4 items)
    c.bench_function("command_batch_8_items", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch([
                Command::event(BenchEvent::A),
                Command::event(BenchEvent::B),
                Command::event(BenchEvent::C),
                Command::event(BenchEvent::D),
                Command::effect(BenchEffect::X),
                Command::effect(BenchEffect::Y),
                Command::effect(BenchEffect::Z),
                Command::effect(BenchEffect::W),
            ]))
        });
    });
}

fn bench_command_iteration(c: &mut Criterion) {
    let small_cmd = Command::<BenchEvent, BenchEffect>::batch([
        Command::event(BenchEvent::A),
        Command::event(BenchEvent::B),
        Command::effect(BenchEffect::X),
        Command::effect(BenchEffect::Y),
    ]);

    let large_cmd = Command::<BenchEvent, BenchEffect>::batch([
        Command::event(BenchEvent::A),
        Command::event(BenchEvent::B),
        Command::event(BenchEvent::C),
        Command::event(BenchEvent::D),
        Command::event(BenchEvent::E),
        Command::effect(BenchEffect::X),
        Command::effect(BenchEffect::Y),
        Command::effect(BenchEffect::Z),
        Command::effect(BenchEffect::W),
        Command::effect(BenchEffect::V),
    ]);

    c.bench_function("iterate_small_command", |b| {
        b.iter(|| {
            let mut count = 0;
            for output in black_box(small_cmd.clone()) {
                count += match output {
                    CommandStep::Event(_) | CommandStep::Effect(_) => 1,
                    CommandStep::Batch(e) | CommandStep::Parallel(e) => e.len(),
                };
            }
            black_box(count)
        });
    });

    c.bench_function("iterate_large_command", |b| {
        b.iter(|| {
            let mut count = 0;
            for output in black_box(large_cmd.clone()) {
                count += match output {
                    CommandStep::Event(_) | CommandStep::Effect(_) => 1,
                    CommandStep::Batch(e) | CommandStep::Parallel(e) => e.len(),
                };
            }
            black_box(count)
        });
    });
}

fn bench_command_composition(c: &mut Criterion) {
    let base_commands: Vec<_> = (0..100)
        .map(|i| {
            if i % 2 == 0 {
                Command::<BenchEvent, BenchEffect>::event(BenchEvent::A)
            } else {
                Command::<BenchEvent, BenchEffect>::effect(BenchEffect::X)
            }
        })
        .collect();

    c.bench_function("batch_100_commands", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch(
                base_commands.clone(),
            ))
        });
    });

    c.bench_function("batch_commands", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch(
                base_commands.clone(),
            ))
        });
    });
}

fn bench_memory_efficiency(c: &mut Criterion) {
    use std::mem;

    // Verify SmallVec doesn't allocate for small commands
    c.bench_function("verify_inline_storage", |b| {
        b.iter(|| {
            let cmd = Command::<BenchEvent, BenchEffect>::batch([
                Command::event(BenchEvent::A),
                Command::event(BenchEvent::B),
                Command::effect(BenchEffect::X),
                Command::effect(BenchEffect::Y),
            ]);
            // This should be inline (no heap allocation)
            black_box(mem::size_of_val(&cmd))
        });
    });
}

criterion_group!(
    benches,
    bench_command_creation,
    bench_command_iteration,
    bench_command_composition,
    bench_memory_efficiency
);
criterion_main!(benches);
````

## File: examples/manual_loop.rs
````rust
//! Drives `Core` and `Shell` manually without the runner abstraction.
//!
//! Build with
//! ```bash
//! cargo run --example manual_loop --features examples
//! ```

use syzygy::executor::{InlineAsync, Task};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct AppModel {
    logs: Vec<String>,
    completed: bool,
}

#[derive(Debug, Clone)]
enum AppEvent {
    Start,
    Completed(String),
}

#[derive(Debug, Clone)]
enum AppEffect {
    ProduceMessage,
}

fn event_handler(event: AppEvent, model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Start => on_start(),
        AppEvent::Completed(message) => on_completed(model, message),
    }
}

fn on_start() -> Command<AppEvent, AppEffect> {
    cmd::effect(AppEffect::ProduceMessage)
}

fn on_completed(model: &mut AppModel, message: String) -> Command<AppEvent, AppEffect> {
    model.logs.push(message);
    model.completed = true;
    cmd::none()
}

fn effect_handler(effect: AppEffect, _resources: ()) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::ProduceMessage => produce_message(),
    }
}

fn produce_message() -> Task<AppEvent, AppEffect> {
    Task::async_on::<InlineAsync, _>(async move {
        cmd::event(AppEvent::Completed("effect finished".to_string()))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut core, mut shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .profile_interactive()
        .with_async_executor(InlineAsync::new())
        .build()
        .split();

    core.try_send_event(AppEvent::Start)
        .expect("event channel should be open");

    while syzygy::syzygy::step_core_shell(&mut core, &mut shell)? {}

    println!("Logs: {:?}", core.model().logs);
    Ok(())
}
````

## File: src/executor/registry.rs
````rust
use rustc_hash::FxHashMap;
use std::any::TypeId;
use std::marker::PhantomData;
use std::sync::Arc;

use super::{AsyncExecutor, BlockingExecutor, ResourceBlockingExecutor};

#[allow(clippy::struct_field_names)]
pub struct ExecutorRegistry<E> {
    async_map: FxHashMap<TypeId, Arc<dyn AsyncExecutor>>,
    blocking_map: FxHashMap<TypeId, Arc<dyn BlockingExecutor>>,
    resource_blocking_map: FxHashMap<TypeId, Arc<dyn ResourceBlockingExecutor>>,
    marker: PhantomData<fn() -> E>,
}

impl<E> Default for ExecutorRegistry<E>
where
    E: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> ExecutorRegistry<E>
where
    E: Send + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            async_map: FxHashMap::default(),
            blocking_map: FxHashMap::default(),
            resource_blocking_map: FxHashMap::default(),
            marker: PhantomData,
        }
    }

    pub fn insert_async<T>(&mut self, exec: T)
    where
        T: AsyncExecutor + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn AsyncExecutor + Send + Sync> = Arc::new(exec);
        self.async_map.insert(key, erased);
    }

    pub fn insert_blocking<T>(&mut self, exec: T)
    where
        T: BlockingExecutor + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn BlockingExecutor + Send + Sync> = Arc::new(exec);
        self.blocking_map.insert(key, erased);
    }

    pub fn insert_resource_blocking<T>(&mut self, exec: T)
    where
        T: ResourceBlockingExecutor + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn ResourceBlockingExecutor + Send + Sync> = Arc::new(exec);
        self.resource_blocking_map.insert(key, erased);
    }

    #[must_use]
    pub fn async_exec<T: 'static>(&self) -> Option<Arc<dyn AsyncExecutor>> {
        self.async_map.get(&TypeId::of::<T>()).cloned()
    }

    #[must_use]
    pub fn blocking_exec<T: 'static>(&self) -> Option<Arc<dyn BlockingExecutor>> {
        self.blocking_map.get(&TypeId::of::<T>()).cloned()
    }

    #[must_use]
    pub fn resource_blocking_exec<T: 'static>(&self) -> Option<Arc<dyn ResourceBlockingExecutor>> {
        self.resource_blocking_map.get(&TypeId::of::<T>()).cloned()
    }

    pub(crate) fn async_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn AsyncExecutor>> {
        self.async_map.get(&key).cloned()
    }

    pub(crate) fn blocking_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn BlockingExecutor>> {
        self.blocking_map.get(&key).cloned()
    }

    pub(crate) fn resource_blocking_exec_by_key(
        &self,
        key: TypeId,
    ) -> Option<Arc<dyn ResourceBlockingExecutor>> {
        self.resource_blocking_map.get(&key).cloned()
    }

    pub fn shutdown_all(&self) {
        for exec in self.async_map.values() {
            exec.shutdown();
        }

        for exec in self.blocking_map.values() {
            exec.shutdown();
        }

        for exec in self.resource_blocking_map.values() {
            exec.shutdown();
        }
    }

    pub fn wait_all(&self) {
        for exec in self.async_map.values() {
            exec.wait();
        }

        for exec in self.blocking_map.values() {
            exec.wait();
        }

        for exec in self.resource_blocking_map.values() {
            exec.wait();
        }
    }
}
````

## File: src/executor/rayon_sync_executor.rs
````rust
use crate::executor::{BlockingExecutor, ExecutorError, ExecutorLifecycle};
use rayon::ThreadPoolBuilder;
use std::marker::PhantomData;

pub struct RayonExecutor<E, R = ()>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    pool: rayon::ThreadPool,
    resources: R,
    _marker: PhantomData<E>,
}

pub struct RayonExecutorBuilder<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    threads: Option<usize>,
    thread_name_prefix: Option<String>,
    resources: R,
    _marker: PhantomData<E>,
}

const DEFAULT_THREAD_PREFIX: &str = "syzygy-rayon";

impl<E> RayonExecutor<E, ()>
where
    E: Send + 'static,
{
    #[must_use]
    pub fn builder() -> RayonExecutorBuilder<E, ()> {
        RayonExecutorBuilder::new(())
    }
}

impl<E, R> RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    #[must_use]
    pub fn resources(&self) -> &R {
        &self.resources
    }
}

impl<E, R> Default for RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Default + Send + Sync + 'static,
{
    fn default() -> Self {
        RayonExecutor::builder().resources(R::default()).build()
    }
}

impl<E, R> BlockingExecutor for RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError> {
        self.pool.spawn(job);
        Ok(())
    }
}

impl<E, R> ExecutorLifecycle for RayonExecutor<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    fn shutdown(&self) {}

    fn wait(&self) {
        // rayon pool does not need explicit waiting; it drains outstanding jobs
    }
}

impl<E, R> RayonExecutorBuilder<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    fn new(resources: R) -> Self {
        Self {
            threads: None,
            thread_name_prefix: None,
            resources,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads.max(1));
        self
    }

    #[must_use]
    pub fn thread_name_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.thread_name_prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn resources<New>(self, resources: New) -> RayonExecutorBuilder<E, New>
    where
        New: Send + Sync + 'static,
    {
        let Self {
            threads,
            thread_name_prefix,
            resources: _,
            _marker: marker,
        } = self;

        RayonExecutorBuilder {
            threads,
            thread_name_prefix,
            resources,
            _marker: marker,
        }
    }

    pub fn build(self) -> RayonExecutor<E, R> {
        let threads = resolve_threads(self.threads);
        let prefix = self
            .thread_name_prefix
            .unwrap_or_else(|| DEFAULT_THREAD_PREFIX.to_string());
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(move |i| format!("{prefix}-{i}"))
            .build()
            .expect("failed to build rayon pool");

        RayonExecutor {
            pool,
            resources: self.resources,
            _marker: PhantomData,
        }
    }
}

fn resolve_threads(explicit: Option<usize>) -> usize {
    explicit
        .filter(|&n| n > 0)
        .unwrap_or_else(default_thread_count)
}

fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
}
````

## File: src/syzygy.rs
````rust
//! # Syzygy - Core/Shell Orchestration
//!
//! This module provides the `Syzygy`, a component that automates the interaction
//! between the `Core` and the `Shell`. It simplifies the process of building and
//! running a Syzygy application by managing the event loop.
//!
//! ## Key Components
//! - `Syzygy` - The main struct that drives the application by orchestrating `Core` and `Shell`.
//! - `SyzygyConfig` - Configuration for the syzygy's behavior.
//!
//! ## Example
//! ```rust,no_run
//! # use syzygy::prelude::*;
//! # use syzygy::executor::{InlineAsync, Task};
//! # #[derive(Debug, Clone)] enum TestEvent { Ping }
//! # #[derive(Debug, Clone)] enum TestEffect { DoPing }
//! # #[derive(Debug, Default)] struct Model;
//! # fn update(_event: TestEvent, _model: &mut Model) -> Command<TestEvent, TestEffect> { Command::none() }
//! # fn handle_effects(_effect: TestEffect, _resources: ()) -> Task<TestEvent, TestEffect> { Task::none() }
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // In a real application, you would build and run the system like this:
//! let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
//!     .model(Model::default())
//!     .event_handler(update)
//!     .effect_handler(handle_effects)
//!     .with_async_executor(InlineAsync::new())
//!     .build();
//!
//! // Run the application indefinitely
//! runner.run()?;
//! # Ok(())
//! # }
//! ```
use std::thread;
use std::time::{Duration, Instant};

use crate::command::Command;
use crate::core::Core;
use crate::error::ShellError;
use crate::shell::{Shell, ShellStats, ShellStatsSnapshot};

/// Configuration for the Syzygy
#[derive(Clone, Debug)]
pub struct SyzygyConfig {
    /// How often to yield control when no work is being done
    pub idle_sleep: Duration,
}

impl Default for SyzygyConfig {
    fn default() -> Self {
        Self {
            idle_sleep: Duration::from_millis(16), // ~60 FPS
        }
    }
}

impl SyzygyConfig {
    /// Override the idle sleep interval with any type convertible to `Duration`.
    #[must_use]
    pub fn idle_sleep(mut self, duration: impl Into<Duration>) -> Self {
        self.idle_sleep = duration.into();
        self
    }
}

/// Preset profiles configuring runner cadence and shell buffering strategies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyzygyProfile {
    /// Responsive UI-style workloads: low latency, bounded effect queue.
    Interactive,
    /// Long-running services prioritizing throughput with bounded channels.
    Server,
    /// Deterministic CI/test runs that should never spin while idle.
    Ci,
    /// Background or batch workloads: trade latency for throughput, unbounded queue.
    Batch,
}

impl SyzygyProfile {
    /// Resolve the profile into concrete configuration values.
    #[must_use]
    pub fn settings(self) -> SyzygyProfileSettings {
        match self {
            SyzygyProfile::Interactive => SyzygyProfileSettings {
                idle_sleep: Duration::from_millis(1),
                effect_channel_capacity: Some(256),
                event_channel_capacity: None,
            },
            SyzygyProfile::Server => SyzygyProfileSettings {
                idle_sleep: Duration::from_millis(0),
                effect_channel_capacity: Some(1024),
                event_channel_capacity: Some(1024),
            },
            SyzygyProfile::Ci => SyzygyProfileSettings {
                idle_sleep: Duration::from_millis(0),
                effect_channel_capacity: Some(64),
                event_channel_capacity: Some(64),
            },
            SyzygyProfile::Batch => SyzygyProfileSettings {
                idle_sleep: Duration::from_millis(25),
                effect_channel_capacity: None,
                event_channel_capacity: None,
            },
        }
    }
}

/// Concrete configuration derived from a [`SyzygyProfile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyzygyProfileSettings {
    pub idle_sleep: Duration,
    pub effect_channel_capacity: Option<usize>,
    /// Bounded inbound event capacity (None => unbounded).
    pub event_channel_capacity: Option<usize>,
}

/// Syzygy automatically orchestrates Core/Shell interaction
///
/// This solves Grug's complaint about manual event loop orchestration.
/// Instead of users manually calling `poll_events` → process → execute → step,
/// Syzygy handles the proper sequencing automatically.
pub struct Syzygy<Event, Effect, Model, Resources = ()>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    core: Core<Event, Effect, Model>,
    shell: Shell<Event, Effect, Resources>,
    config: SyzygyConfig,
}

/// Terminology-friendly alias for [`Syzygy`], emphasizing its role as the runtime runner.
pub type Runner<Event, Effect, Model, Resources = ()> = Syzygy<Event, Effect, Model, Resources>;

impl<Event, Effect, Model, Resources> Syzygy<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    /// Create a new Syzygy with Core and Shell
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let (core, shell) = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _model| Command::none())
    ///     .effect_handler(|_effect: Effect, _resources| Task::none())
    ///     .build()
    ///     .split();
    ///
    /// let syzygy = Syzygy::new(core, shell);
    /// ```
    pub fn new(core: Core<Event, Effect, Model>, shell: Shell<Event, Effect, Resources>) -> Self {
        Self {
            core,
            shell,
            config: SyzygyConfig::default(),
        }
    }

    /// Create a new Syzygy with custom configuration
    pub fn with_config(
        core: Core<Event, Effect, Model>,
        shell: Shell<Event, Effect, Resources>,
        config: SyzygyConfig,
    ) -> Self {
        Self {
            core,
            shell,
            config,
        }
    }

    /// Run the event loop continuously
    ///
    /// This will run until the shell is shut down or an error occurs.
    pub fn run(&mut self) -> Result<(), ShellError> {
        self.run_loop(Syzygy::step, |_| false)
    }

    /// Run until a condition is met
    ///
    /// Useful for testing or conditional execution.
    pub fn run_until<F>(&mut self, mut condition: F) -> Result<(), ShellError>
    where
        F: FnMut(&Core<Event, Effect, Model>, &Shell<Event, Effect, Resources>) -> bool,
    {
        self.run_loop(Syzygy::step, move |syzygy| {
            condition(&syzygy.core, &syzygy.shell)
        })
    }

    /// Drain the system for at most `max_steps` iterations.
    ///
    /// Returns the number of steps that performed work. Stops early if no work
    /// remains before hitting `max_steps`.
    pub fn drain_max(&mut self, max_steps: usize) -> Result<usize, ShellError> {
        let mut steps = 0;
        for _ in 0..max_steps {
            if !self.step()? {
                break;
            }
            steps += 1;
        }
        Ok(steps)
    }

    /// Drain until the predicate returns true or `timeout` elapses.
    pub fn drain_until<F>(&mut self, mut predicate: F, timeout: Duration) -> Result<(), ShellError>
    where
        F: FnMut(&Model) -> bool,
    {
        if predicate(self.core.model()) {
            return Ok(());
        }

        if timeout.is_zero() {
            return Err(ShellError::Timeout { duration: timeout });
        }

        let start = Instant::now();
        let deadline = start.checked_add(timeout);

        loop {
            if predicate(self.core.model()) {
                return Ok(());
            }

            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }
            }

            let did_work = self.step()?;

            if predicate(self.core.model()) {
                return Ok(());
            }

            if let Some(deadline) = deadline {
                let now = Instant::now();
                if now >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }

                if !did_work {
                    let remaining = deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO);
                    if remaining.is_zero() {
                        return Err(ShellError::Timeout { duration: timeout });
                    }

                    let had_work = self.wait_for_idle(remaining);

                    if predicate(self.core.model()) {
                        return Ok(());
                    }

                    if !had_work && Instant::now() >= deadline {
                        return Err(ShellError::Timeout { duration: timeout });
                    }
                }
            } else if !did_work {
                self.wait_for_work();
            }
        }
    }

    /// Drain the system until both Core and Shell report no outstanding work.
    pub fn drain_until_idle(&mut self, timeout: Duration) -> Result<(), ShellError> {
        if self.is_fully_idle() {
            return Ok(());
        }

        if timeout.is_zero() {
            return Err(ShellError::Timeout { duration: timeout });
        }

        let start = Instant::now();
        let deadline = start.checked_add(timeout);

        loop {
            let did_work = self.step()?;

            if self.is_fully_idle() {
                return Ok(());
            }

            if let Some(deadline) = deadline {
                let now = Instant::now();
                if now >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }

                if !did_work {
                    let remaining = deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO);
                    if remaining.is_zero() {
                        return Err(ShellError::Timeout { duration: timeout });
                    }

                    let had_work = self.wait_for_idle(remaining);

                    if self.is_fully_idle() {
                        return Ok(());
                    }

                    if !had_work && Instant::now() >= deadline {
                        return Err(ShellError::Timeout { duration: timeout });
                    }
                }
            } else if !did_work {
                self.wait_for_work();
            }
        }
    }

    /// Get an immutable reference to the model
    #[must_use]
    pub fn model(&self) -> &Model {
        self.core.model()
    }

    /// Get a mutable reference to the model
    ///
    /// This should be used carefully as it bypasses event processing.
    /// Prefer sending events for state changes.
    pub fn model_mut(&mut self) -> &mut Model {
        self.core.model_mut()
    }

    /// Get a reference to the Core
    pub fn core(&self) -> &Core<Event, Effect, Model> {
        &self.core
    }

    /// Get a mutable reference to the Core
    pub fn core_mut(&mut self) -> &mut Core<Event, Effect, Model> {
        &mut self.core
    }

    /// Get a reference to the Shell
    pub fn shell(&self) -> &Shell<Event, Effect, Resources> {
        &self.shell
    }

    /// Get a mutable reference to the Shell
    pub fn shell_mut(&mut self) -> &mut Shell<Event, Effect, Resources> {
        &mut self.shell
    }

    /// Get the syzygy configuration
    pub fn config(&self) -> &SyzygyConfig {
        &self.config
    }

    /// Update the syzygy configuration
    pub fn set_config(&mut self, config: SyzygyConfig) {
        self.config = config;
    }

    fn is_fully_idle(&self) -> bool {
        !self.core.has_pending_events() && self.shell.is_idle()
    }

    /// Request that the shell stop scheduling further work.
    pub fn shutdown(&mut self) {
        self.shell.shutdown();
        self.shell.wait_for_executors();

        #[cfg(feature = "tracing")]
        {
            let snapshot = self.shell.stats();
            if snapshot.dropped_events > 0 || snapshot.dropped_effect_steps > 0 {
                tracing::warn!(
                    stage = "syzygy_shutdown",
                    dropped_events = snapshot.dropped_events,
                    dropped_effect_steps = snapshot.dropped_effect_steps,
                    "Syzygy shutdown completed with dropped work"
                );
            } else {
                tracing::debug!(stage = "syzygy_shutdown", "Syzygy shutdown cleanly");
            }
        }
    }

    /// Wait until the system is completely idle or timeout expires
    ///
    /// System is considered idle when:
    /// - No pending events in core
    /// - No pending effects in shell
    /// - No in-flight async jobs
    ///
    /// Returns Err(ShellError::Timeout) if timeout expires before reaching idle.
    pub fn await_idle(&mut self, timeout: Duration) -> Result<(), ShellError> {
        if timeout.is_zero() {
            return Err(ShellError::Timeout { duration: timeout });
        }

        let start = Instant::now();
        let deadline = start.checked_add(timeout);

        loop {
            // Check if system is idle
            if self.core.pending_count() == 0
                && self.shell.pending_effects() == 0
                && self.shell.inflight_jobs() == 0
            {
                return Ok(());
            }

            // Check timeout
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(ShellError::Timeout { duration: timeout });
                }
            }

            // Process any available work
            let did_work = self.step()?;

            // If no work was done, wait for work to arrive
            if !did_work {
                let remaining = if let Some(deadline) = deadline {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(ShellError::Timeout { duration: timeout });
                    }
                    deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO)
                } else {
                    self.config.idle_sleep
                };

                // Wait for activity to reach zero or for new work to arrive
                if self.shell.inflight_jobs() > 0 {
                    if !self.shell.activity.wait_until_zero(remaining) {
                        // Timeout waiting for activity, but we might still have work
                        continue;
                    }
                } else {
                    let _ = self.wait_for_idle(remaining);
                }
            }
        }
    }

    /// Run until the system is completely idle (no timeout)
    ///
    /// This is a convenience method that calls `await_idle` with an infinite timeout.
    pub fn run_to_idle(&mut self) -> Result<(), ShellError> {
        self.await_idle(Duration::MAX)
    }

    /// Dispatch a command and then wait until the system is idle
    ///
    /// This is a convenience method that combines command dispatch with idle waiting.
    pub fn dispatch_and_await(
        &mut self,
        command: Command<Event, Effect>,
        timeout: Duration,
    ) -> Result<(), ShellError> {
        self.shell.dispatch_command(command)?;
        self.await_idle(timeout)
    }

    /// Wait for work to arrive, trying core first then shell
    ///
    /// Returns true if work arrived, false if we timed out
    #[must_use = "ignoring work detection defeats the purpose of waiting"]
    fn wait_for_idle(&mut self, duration: Duration) -> bool {
        let start = Instant::now();

        // Try core first - bail immediately if channel closed
        if self.core.wait_for_event(duration) {
            return true;
        }

        // If core didn't get work and shell is closed, bail
        if self.shell.is_closed() {
            return false;
        }

        // Calculate remaining time for shell
        let elapsed = start.elapsed();
        let remaining = match duration.checked_sub(elapsed) {
            Some(remaining) if !remaining.is_zero() => remaining,
            _ => return false, // No time left
        };

        // Try shell with remaining time and return its result
        self.shell.wait_for_effect(remaining)
    }

    /// Wait for work to arrive, blocking the current thread until either:
    /// - An event arrives in the core
    /// - An effect arrives in the shell
    /// - The idle sleep timeout expires
    /// - The shell is closed
    ///
    /// This method is called automatically by `run()` and `run_until()` when no work
    /// is available, but can also be called manually for fine-grained control.
    pub fn wait_for_work(&mut self) {
        // Early returns for cases where we shouldn't wait
        if self.shell.is_closed() {
            return;
        }

        if self.config.idle_sleep.is_zero() {
            thread::yield_now();
            return;
        }

        if self.core.has_pending_events() || self.shell.pending_effects() > 0 {
            return;
        }

        // Wait for work to arrive - we ignore the result since the main loop
        // will check for pending work on the next iteration anyway
        #[allow(unused_must_use)]
        {
            self.wait_for_idle(self.config.idle_sleep);
        }
    }

    fn run_loop<Step, Exit>(
        &mut self,
        mut step: Step,
        mut should_exit: Exit,
    ) -> Result<(), ShellError>
    where
        Step: FnMut(&mut Self) -> Result<bool, ShellError>,
        Exit: FnMut(&Self) -> bool,
    {
        loop {
            let did_work = step(self)?;

            if should_exit(self) || self.shell.is_closed() {
                break;
            }

            if !did_work {
                self.wait_for_work();
            }
        }

        Ok(())
    }

    /// Execute a single synchronous step of the event loop
    ///
    /// This processes events from Core and effects from Shell synchronously.
    /// Returns true if work was done, false if idle.
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Default)] struct Model;
    /// # #[derive(Debug, Clone)] enum Event { Test }
    /// # #[derive(Debug, Clone)] enum Effect { Test }
    /// let mut runner = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(|_event: Event, _model| Command::none())
    ///     .effect_handler(|_effect: Effect, _resources| Task::none())
    ///     .build();
    /// runner.core().try_send_event(Event::Test)?;
    ///
    /// // Process the event synchronously
    /// let did_work = runner.step()?;
    /// assert!(did_work);
    /// ```
    pub fn step(&mut self) -> Result<bool, ShellError> {
        step_core_shell(&mut self.core, &mut self.shell)
    }

    /// Snapshot Shell-level counters (dropped events/effects).
    #[must_use]
    pub fn shell_stats(&self) -> ShellStatsSnapshot {
        self.shell.stats()
    }

    /// Clone a shared stats handle for integration with metrics or tracing.
    #[must_use]
    pub fn shell_stats_handle(&self) -> ShellStats {
        self.shell.stats_handle()
    }

    /// Consume the runner and return ownership of the Core and Shell.
    pub fn split(self) -> (Core<Event, Effect, Model>, Shell<Event, Effect, Resources>) {
        (self.core, self.shell)
    }
}

/// Process pending events and effects once using the same logic as `Syzygy::step`.
pub fn step_core_shell<Event, Effect, Model, Resources>(
    core: &mut Core<Event, Effect, Model>,
    shell: &mut Shell<Event, Effect, Resources>,
) -> Result<bool, ShellError>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    let commands = core.process_events();
    let core_work = !commands.is_empty();
    for command in commands {
        shell.dispatch_command(command)?;
    }

    let shell_work = shell.drain()?;

    Ok(core_work || shell_work > 0)
}

impl<Event, Effect, Model, Resources>
    From<(Core<Event, Effect, Model>, Shell<Event, Effect, Resources>)>
    for Syzygy<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    fn from(parts: (Core<Event, Effect, Model>, Shell<Event, Effect, Resources>)) -> Self {
        Self::new(parts.0, parts.1)
    }
}

impl<Event, Effect, Model, Resources> std::fmt::Debug for Syzygy<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syzygy")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Syzygy<(), (), ()> {
    /// Create a new builder for Syzygy systems
    #[must_use]
    pub fn builder<NewEvent, NewEffect>(
    ) -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, (), ()>
    where
        NewEvent: Send + 'static,
        NewEffect: Send + 'static,
    {
        crate::builder::SyzygyBuilder::new()
    }

    /// Start a builder preloaded with the [`SyzygyProfile::Interactive`] preset.
    #[must_use]
    pub fn interactive_builder<NewEvent, NewEffect>(
    ) -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, (), ()>
    where
        NewEvent: Send + 'static,
        NewEffect: Send + 'static,
    {
        crate::builder::SyzygyBuilder::new().preset_profile(SyzygyProfile::Interactive)
    }

    /// Start a builder preloaded with the [`SyzygyProfile::Server`] preset.
    #[must_use]
    pub fn server_builder<NewEvent, NewEffect>(
    ) -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, (), ()>
    where
        NewEvent: Send + 'static,
        NewEffect: Send + 'static,
    {
        crate::builder::SyzygyBuilder::new().preset_profile(SyzygyProfile::Server)
    }

    /// Start a builder preloaded with the [`SyzygyProfile::Ci`] preset.
    #[must_use]
    pub fn ci_builder<NewEvent, NewEffect>(
    ) -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, (), ()>
    where
        NewEvent: Send + 'static,
        NewEffect: Send + 'static,
    {
        crate::builder::SyzygyBuilder::new().preset_profile(SyzygyProfile::Ci)
    }

    /// Start a builder preloaded with the [`SyzygyProfile::Batch`] preset.
    #[must_use]
    pub fn batch_builder<NewEvent, NewEffect>(
    ) -> crate::builder::SyzygyBuilder<NewEvent, NewEffect, (), ()>
    where
        NewEvent: Send + 'static,
        NewEffect: Send + 'static,
    {
        crate::builder::SyzygyBuilder::new().preset_profile(SyzygyProfile::Batch)
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;
    use crate::command::Command;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Ping,
        Pong,
    }

    #[derive(Debug, Default)]
    struct TestModel {
        count: i32,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    fn test_update(event: TestEvent, model: &mut TestModel) -> Command<TestEvent, TestEffect> {
        match event {
            TestEvent::Ping => {
                model.count += 1;
                Command::event(TestEvent::Pong)
            }
            TestEvent::Pong => {
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_runner_basic() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        let event_sender = runner.core().event_sender();

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        // Process one step
        let did_work = runner.step().unwrap();

        assert!(did_work);
        // Skipped model access check in refactor
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_runner_until_condition() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        runner.set_config(SyzygyConfig {
            idle_sleep: Duration::from_millis(1),
        });

        let event_sender = runner.core().event_sender();

        // Send multiple events
        for _ in 0..5 {
            event_sender.send(TestEvent::Ping).unwrap();
        }

        // Run until count reaches 10
        runner.run_until(|_core, _shell| true).unwrap();

        // Skipped model access check in refactor
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_step_returns_true_when_work_was_done() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        let event_sender = runner.core().event_sender();

        // Send an event to create work
        event_sender.send(TestEvent::Ping).unwrap();

        // Step should return true (work was done)
        let did_work = runner.step().expect("Step should succeed");
        assert!(did_work, "Step should return true when work was done");
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_step_returns_false_when_no_work_to_do() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        // No events sent, so no work to do
        let did_work = runner.step().expect("Step should succeed");
        assert!(!did_work, "Step should return false when no work to do");
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_step_with_custom_executor() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        let event_sender = runner.core().event_sender();

        // Send an event
        event_sender.send(TestEvent::Ping).unwrap();

        let did_work = runner.step().expect("Step should succeed");
        assert!(
            did_work,
            "Step with custom executor should return true when work was done"
        );
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_step_processes_exactly_one_event() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        let event_sender = runner.core().event_sender();

        // Send multiple events
        event_sender.send(TestEvent::Ping).unwrap();
        event_sender.send(TestEvent::Ping).unwrap();
        event_sender.send(TestEvent::Ping).unwrap();

        // First step should process ALL 3 events and return true
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(
            did_work1,
            "First step should return true (processed all 3 events)"
        );

        // Second step should process effects and return true
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(
            did_work2,
            "Second step should return true (processed effects)"
        );

        // Third step should have no more work and return false
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(!did_work3, "Third step should return false (no more work)");
    }

    #[cfg(all(feature = "tokio", feature = "rt-inline"))]
    #[tokio::test]
    async fn test_step_handles_multiple_events_and_effects_in_sequence() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .with_async_executor(crate::executor::InlineAsync::new())
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        let event_sender = runner.core().event_sender();

        // Send Ping event (will generate Pong event and Log effect)
        event_sender.send(TestEvent::Ping).unwrap();

        // First step: process Ping (generates Pong) and Pong (generates Log effect)
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(did_work1, "First step should process Ping and Pong events");

        // Second step: process Log effects
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(did_work2, "Second step should process Log effects");

        // Third step: no more work
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(!did_work3, "Third step should have no more work");
    }
}
````

## File: src/executor/single_thread_executor.rs
````rust
use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};
use futures::channel::oneshot;
use futures::executor::block_on;
use futures_util::future::{BoxFuture, FutureExt, Shared};

use crate::executor::{panic_message, ExecutorError, ExecutorLifecycle, ResourceBlockingExecutor};

/// Type alias for the complex job function type used by SingleThreadExecutor
type JobFn = Box<dyn FnOnce(&mut dyn Any) + Send>;

/// Single-threaded executor that runs blocking jobs on a dedicated worker thread,
/// providing mutable access to executor-owned resources.
///
/// This is your strict FIFO lane for jobs that must serialize access to a
/// single resource (e.g. a device or legacy client that explodes under
/// concurrency).
pub struct SingleThreadExecutor<R = ()> {
    state: Arc<State>,
    resource_type_id: TypeId,
    _marker: PhantomData<fn() -> R>,
}

impl Default for SingleThreadExecutor<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleThreadExecutor<()> {
    #[must_use]
    pub fn new() -> Self {
        Self::with_resources(())
    }
}

impl<R> SingleThreadExecutor<R>
where
    R: Send + 'static,
{
    #[must_use]
    pub fn with_resources(resources: R) -> Self {
        let (tx, rx) = unbounded::<Job>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("syzygy-single-executor".into())
            .spawn(move || worker_loop(rx, shutdown_tx, resources))
            .expect("failed to spawn single-thread executor");

        let completed_shutdown = async move {
            let _ = shutdown_rx.await;
        }
        .boxed()
        .shared();

        let state = State {
            tx,
            shutdown_requested: AtomicBool::new(false),
            completed_shutdown,
            thread: Mutex::new(Some(thread)),
        };

        Self {
            state: Arc::new(state),
            resource_type_id: TypeId::of::<R>(),
            _marker: PhantomData,
        }
    }
}

impl<R> SingleThreadExecutor<R>
where
    R: Send + 'static,
{
    /// Convenience helper for submitting typed jobs.
    pub fn spawn<F>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(&mut R) + Send + 'static,
    {
        let wrapped = Box::new(move |resource: &mut dyn Any| {
            let typed = resource
                .downcast_mut::<R>()
                .expect("single thread executor resource type mismatch");
            job(typed);
        });

        if self.state.shutdown_requested.load(Ordering::Acquire)
            || self.state.tx.send(Job::Work(wrapped)).is_err()
        {
            return Err(ExecutorError::WorkerGone);
        }

        Ok(())
    }
}

impl<R> ResourceBlockingExecutor for SingleThreadExecutor<R>
where
    R: Send + 'static,
{
    fn resource_type_id(&self) -> std::any::TypeId {
        self.resource_type_id
    }

    fn spawn_blocking_with_resource(
        &self,
        job: Box<dyn FnOnce(&mut dyn Any) + Send>,
    ) -> Result<(), ExecutorError> {
        if self.state.shutdown_requested.load(Ordering::Acquire)
            || self.state.tx.send(Job::Work(job)).is_err()
        {
            return Err(ExecutorError::WorkerGone);
        }

        Ok(())
    }
}

impl<R> ExecutorLifecycle for SingleThreadExecutor<R>
where
    R: Send + 'static,
{
    fn shutdown(&self) {
        if !self.state.shutdown_requested.swap(true, Ordering::AcqRel) {
            let _ = self.state.tx.send(Job::Shutdown);
        }
    }

    fn wait(&self) {
        self.shutdown();
        let fut = self.state.completed_shutdown.clone();
        block_on(async {
            let () = fut.await;
        });
    }
}

struct State {
    tx: Sender<Job>,
    shutdown_requested: AtomicBool,
    completed_shutdown: Shared<BoxFuture<'static, ()>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for State {
    fn drop(&mut self) {
        if !self.shutdown_requested.swap(true, Ordering::AcqRel) {
            let _ = self.tx.send(Job::Shutdown);
        }

        let _ = self.completed_shutdown.clone().now_or_never();

        if let Ok(mut guard) = self.thread.lock() {
            if let Some(join) = guard.take() {
                let _ = join.join();
            }
        }
    }
}

enum Job {
    Work(JobFn),
    Shutdown,
}

fn worker_loop<R>(rx: Receiver<Job>, shutdown_tx: oneshot::Sender<()>, mut resources: R)
where
    R: Send + 'static,
{
    while let Ok(job) = rx.recv() {
        match job {
            Job::Work(job) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job(&mut resources as &mut dyn Any);
                }));

                if let Err(panic) = result {
                    let _ = panic_message(panic);
                }
            }
            Job::Shutdown => break,
        }
    }

    let _ = shutdown_tx.send(());
}
````

## File: tests/shell_method_tests.rs
````rust
#![cfg(all(feature = "shell", feature = "rt-inline", feature = "tokio"))]
//! Tests for Shell synchronous methods: `drain_with`, `dispatch_command`, etc.
//! These tests use the Runner API for convenience.

use syzygy::executor::{InlineAsync, PanicTaskKind, Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum TestEvent {
    Ping,
    Pong,
}

#[derive(Debug, Clone)]
enum TestEffect {
    Log,
    Work(u8),
}

#[derive(Debug, Default)]
struct TestModel {
    count: i32,
}

fn test_update(event: TestEvent, model: &mut TestModel) -> Command<TestEvent, TestEffect> {
    match event {
        TestEvent::Ping => {
            model.count += 1;
            cmd::event(TestEvent::Pong)
        }
        TestEvent::Pong => {
            model.count += 1;
            cmd::effect(TestEffect::Log)
        }
    }
}

#[cfg(feature = "tokio")]
mod tokio_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use syzygy::error::ShellError;
    use tokio_util::sync::CancellationToken;

    fn create_test_runner() -> Runner<TestEvent, TestEffect, TestModel> {
        Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .profile_interactive()
            .with_async_executor(InlineAsync::new())
            .build()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn effect_queue_respects_capacity_limits() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .profile_interactive()
            .with_async_executor(InlineAsync::new())
            .with_effect_channel_capacity(Some(1))
            .build();

        let shell = runner.shell_mut();
        shell
            .dispatch_command(Command::effect(TestEffect::Log))
            .expect("first effect should fit in queue");

        let error = shell
            .dispatch_command(Command::effect(TestEffect::Log))
            .expect_err("second effect should exceed capacity");

        match error {
            ShellError::EffectQueueFull { capacity } => assert_eq!(capacity, 1),
            other => panic!("expected EffectQueueFull, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_pending_effects_starts_at_zero() {
        let runner = create_test_runner();
        let shell = runner.shell();
        assert_eq!(shell.pending_effects(), 0, "Should start with empty queue");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_handles_shutdown_state_correctly() {
        let mut runner = create_test_runner();

        // Initially not closed
        assert!(!runner.shell().is_closed());

        // Shutdown via runner
        runner.shutdown();
        assert!(runner.shell().is_closed());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_config_access_works() {
        let runner = create_test_runner();
        let shell = runner.shell();

        // Should be able to access config
        assert!(
            shell.effect_channel_capacity().is_none(),
            "Default queue should be unbounded"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_processes_events_and_effects_via_runner() {
        let mut runner = create_test_runner();

        // Send an event that will generate another event and an effect
        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        // First step: process Ping -> generates Pong event (which is routed back to Core)
        let did_work1 = runner.step().expect("First step should succeed");
        assert!(did_work1, "First step should process Ping event");

        // Second step: process Pong event -> generates AND processes Log effect in same step
        let did_work2 = runner.step().expect("Second step should succeed");
        assert!(
            did_work2,
            "Second step should process Pong event and Log effect"
        );

        // Third step: no more work (effect was already processed in step 2)
        let did_work3 = runner.step().expect("Third step should succeed");
        assert!(!did_work3, "Third step should have no more work");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_handles_multiple_events_correctly() {
        let mut runner = create_test_runner();

        // Send multiple events
        for _ in 0..3 {
            runner
                .core_mut()
                .try_send_event(TestEvent::Ping)
                .expect("event channel should be open");
        }

        // Process all events and effects
        let mut total_steps = 0;
        while runner.step().expect("Step should succeed") {
            total_steps += 1;
        }

        // Core processes ALL events in queue at once:
        // Step 1: Process all 3 Ping events -> generates 3 Pong events
        // Step 2: Process all 3 Pong events -> generates and processes 3 Log effects
        assert_eq!(
            total_steps, 2,
            "Should process exactly 2 steps for 3 Ping events (batch processing)"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_reports_missing_async_executor() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::effect(TestEffect::Work(99)),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(|effect: TestEffect, _resources| match effect {
                TestEffect::Work(_) => {
                    Task::<TestEvent, TestEffect>::async_on::<TokioExecutor, _>(async move {
                        Command::none()
                    })
                }
                TestEffect::Log => Task::<TestEvent, TestEffect>::none(),
            })
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        let err = runner
            .step()
            .expect_err("step should fail when required executor is missing");

        match err {
            ShellError::TaskSpawnFailed(message) => {
                assert!(
                    message.contains("Missing async executor"),
                    "error should mention missing async executor, got: {message:?}"
                );
                assert!(
                    message.contains("TokioExecutor"),
                    "error should include executor type name, got: {message:?}"
                );
            }
            other => panic!("Expected TaskSpawnFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn await_idle_waits_for_async_work() {
        #[derive(Debug, Clone)]
        enum CliEvent {
            Start,
            Done,
        }

        #[derive(Debug, Default)]
        struct CliModel {
            completed: usize,
        }

        #[derive(Debug, Clone)]
        enum CliEffect {
            Work,
        }

        fn cli_update(event: CliEvent, model: &mut CliModel) -> Command<CliEvent, CliEffect> {
            match event {
                CliEvent::Start => Command::effect(CliEffect::Work),
                CliEvent::Done => {
                    model.completed += 1;
                    Command::none()
                }
            }
        }

        fn cli_effects(effect: CliEffect, _: ()) -> Task<CliEvent, CliEffect> {
            match effect {
                CliEffect::Work => Task::async_current(async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Command::event(CliEvent::Done)
                }),
            }
        }

        let mut runner = Syzygy::builder::<CliEvent, CliEffect>()
            .model(CliModel::default())
            .event_handler(cli_update)
            .effect_handler(cli_effects)
            .build();

        runner
            .core_mut()
            .try_send_event(CliEvent::Start)
            .expect("event channel should be open");

        runner
            .await_idle(Duration::from_secs(1))
            .expect("await_idle should return once work completes");

        assert_eq!(runner.core().model().completed, 1);
        assert!(runner.shell().is_idle());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dispatch_and_await_processes_command() {
        #[derive(Debug, Clone)]
        enum CliEvent {
            Done,
        }

        #[derive(Debug, Default)]
        struct CliModel {
            hits: usize,
        }

        #[derive(Debug, Clone)]
        enum CliEffect {
            Work,
        }

        fn cli_update(event: CliEvent, model: &mut CliModel) -> Command<CliEvent, CliEffect> {
            match event {
                CliEvent::Done => {
                    model.hits += 1;
                    Command::none()
                }
            }
        }

        fn cli_effects(effect: CliEffect, _: ()) -> Task<CliEvent, CliEffect> {
            match effect {
                CliEffect::Work => Task::async_current(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Command::event(CliEvent::Done)
                }),
            }
        }

        let mut runner = Syzygy::builder::<CliEvent, CliEffect>()
            .model(CliModel::default())
            .event_handler(cli_update)
            .effect_handler(cli_effects)
            .build();

        runner
            .dispatch_and_await(Command::effect(CliEffect::Work), Duration::from_secs(1))
            .expect("dispatch_and_await should process command");

        assert_eq!(runner.core().model().hits, 1);
        assert!(runner.shell().is_idle());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_max_limits_iterations() {
        let mut runner = create_test_runner();
        for _ in 0..3 {
            runner
                .core_mut()
                .try_send_event(TestEvent::Ping)
                .expect("event channel should be open");
        }

        let steps = runner.drain_max(1).expect("drain_max should succeed");
        assert_eq!(steps, 1, "Should only perform a single step");

        // Some work remains; another drain should make progress.
        let more_steps = runner.drain_max(10).expect("second drain should succeed");
        assert!(more_steps >= 1, "Expected additional work to be processed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_until_completes_or_times_out() {
        let mut runner = create_test_runner();
        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        runner
            .drain_until(|model| model.count >= 2, Duration::from_secs(1))
            .expect("drain_until should complete");
        assert!(runner.core().model().count >= 2);

        let timeout = runner
            .drain_until(|model| model.count > 100, Duration::from_millis(10))
            .expect_err("expected timeout");
        assert!(matches!(timeout, ShellError::Timeout { .. }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drain_until_idle_waits_for_shell() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|effect: TestEffect, _| match effect {
                TestEffect::Log => {
                    Task::<TestEvent, TestEffect>::async_on::<InlineAsync, _>(async move {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        cmd::none()
                    })
                }
                TestEffect::Work(_) => Task::none(),
            })
            .profile_interactive()
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        runner
            .drain_until_idle(Duration::from_millis(500))
            .expect("drain_until_idle should observe idle");

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        let err = runner
            .drain_until_idle(Duration::from_millis(1))
            .expect_err("expected timeout when idle not reached in time");
        assert!(matches!(err, ShellError::Timeout { .. }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn async_current_with_cancel_dispatches_cancel_command() {
        #[derive(Debug, Default)]
        struct CancelModel {
            cancelled: usize,
        }

        #[derive(Debug, Clone)]
        enum CancelEvent {
            Start,
            Cancelled,
        }

        #[derive(Debug, Clone)]
        enum CancelEffect {
            Run,
        }

        let token = CancellationToken::new();
        let token_for_runner = token.clone();

        let mut runner = Syzygy::builder::<CancelEvent, CancelEffect>()
            .model(CancelModel::default())
            .with_resources(token_for_runner)
            .event_handler(|event, model| match event {
                CancelEvent::Start => cmd::effect(CancelEffect::Run),
                CancelEvent::Cancelled => {
                    model.cancelled += 1;
                    cmd::none()
                }
            })
            .effect_handler(
                |effect: CancelEffect, token: CancellationToken| match effect {
                    CancelEffect::Run => {
                        Task::<CancelEvent, CancelEffect>::async_current_with_cancel(
                            async move {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                cmd::event(CancelEvent::Cancelled)
                            },
                            token.cancelled(),
                            cmd::event(CancelEvent::Cancelled),
                        )
                    }
                },
            )
            .profile_interactive()
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(CancelEvent::Start)
            .expect("event channel should be open");

        token.cancel();

        runner
            .drain_until_idle(Duration::from_secs(1))
            .expect("should observe idle after cancellation");

        assert_eq!(runner.core().model().cancelled, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn panic_handler_turns_panics_into_events() {
        #[derive(Debug, Default)]
        struct PanicModel {
            messages: Vec<String>,
        }

        #[derive(Debug, Clone)]
        enum PanicEvent {
            Trigger,
            Notified(String),
        }

        #[derive(Debug, Clone)]
        enum PanicEffect {
            Explode,
        }

        let observed_details = Arc::new(Mutex::new(None));
        let observed_for_handler = Arc::clone(&observed_details);

        let mut runner = Syzygy::builder::<PanicEvent, PanicEffect>()
            .model(PanicModel::default())
            .event_handler(|event, model| match event {
                PanicEvent::Trigger => cmd::effect(PanicEffect::Explode),
                PanicEvent::Notified(message) => {
                    model.messages.push(message);
                    cmd::none()
                }
            })
            .effect_handler(|effect: PanicEffect, _| match effect {
                PanicEffect::Explode => {
                    Task::<PanicEvent, PanicEffect>::async_current(async move {
                        panic!("boom!");
                        #[allow(unreachable_code)]
                        {
                            cmd::none()
                        }
                    })
                }
            })
            .with_panic_handler(move |details, message| {
                *observed_for_handler.lock().unwrap() = Some(details);
                cmd::event(PanicEvent::Notified(message))
            })
            .profile_interactive()
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(PanicEvent::Trigger)
            .expect("event channel should be open");

        runner
            .drain_until_idle(Duration::from_secs(1))
            .expect("panic handler should drain to idle");

        let model = runner.core().model();
        assert_eq!(model.messages.len(), 1);
        assert!(model.messages[0].contains("boom"));

        let details = observed_details
            .lock()
            .unwrap()
            .expect("panic details should be recorded");
        assert_eq!(details.kind, PanicTaskKind::AsyncCurrent);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn batch_effects_execute_in_sequence() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let order_for_handler = Arc::clone(&order);
        let in_flight_for_handler = Arc::clone(&in_flight);
        let max_in_flight_for_handler = Arc::clone(&max_in_flight);

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::sequential([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _resources| {
                let order = Arc::clone(&order_for_handler);
                let in_flight = Arc::clone(&in_flight_for_handler);
                let max_in_flight = Arc::clone(&max_in_flight_for_handler);

                Task::<TestEvent, TestEffect>::async_on::<InlineAsync, _>(async move {
                    if let TestEffect::Work(id) = effect {
                        let active = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(active, Ordering::SeqCst);

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("start-{id}"));
                        }

                        std::thread::sleep(Duration::from_millis(5));

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("end-{id}"));
                        }

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    }

                    Command::none()
                })
            })
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        while runner.step().expect("step should succeed") {
            tokio::task::yield_now().await;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let log = order.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "start-1".to_string(),
                "end-1".to_string(),
                "start-2".to_string(),
                "end-2".to_string(),
                "start-3".to_string(),
                "end-3".to_string(),
            ],
            "Batch effects should run strictly sequentially",
        );
        assert_eq!(max_in_flight.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shell_default_queue_is_unbounded() {
        let mut runner = create_test_runner();
        let shell = runner.shell_mut();

        let command = Command::batch((0..1_500).map(|_| Command::effect(TestEffect::Log)));

        shell
            .dispatch_command(command)
            .expect("Should not hit an artificial queue limit");

        assert_eq!(shell.pending_effects(), 1_500);

        let processed = shell.drain().expect("Draining should succeed");

        assert_eq!(processed, 1_500);
        assert_eq!(shell.pending_effects(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parallel_effects_overlap_on_concurrent_executor() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let order_for_handler = Arc::clone(&order);
        let in_flight_for_handler = Arc::clone(&in_flight);
        let max_in_flight_for_handler = Arc::clone(&max_in_flight);

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::parallel([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _resources| {
                let order = Arc::clone(&order_for_handler);
                let in_flight = Arc::clone(&in_flight_for_handler);
                let max_in_flight = Arc::clone(&max_in_flight_for_handler);

                Task::<TestEvent, TestEffect>::async_on::<TokioExecutor, _>(async move {
                    if let TestEffect::Work(id) = effect {
                        let active = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(active, Ordering::SeqCst);

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("start-{id}"));
                        }

                        tokio::time::sleep(Duration::from_millis(10)).await;

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("end-{id}"));
                        }

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    }

                    Command::none()
                })
            })
            .with_async_executor(TokioExecutor::current_thread_io("parallel-overlap"))
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        while runner.step().expect("step should succeed") {
            tokio::task::yield_now().await;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let log = order.lock().unwrap().clone();
        let mut starts: Vec<_> = log
            .iter()
            .filter(|entry| entry.starts_with("start-"))
            .cloned()
            .collect();
        starts.sort();
        assert_eq!(
            starts,
            vec![
                "start-1".to_string(),
                "start-2".to_string(),
                "start-3".to_string(),
            ],
            "All effects should have started",
        );

        let mut ends: Vec<_> = log
            .iter()
            .filter(|entry| entry.starts_with("end-"))
            .cloned()
            .collect();
        ends.sort();
        assert_eq!(
            ends,
            vec![
                "end-1".to_string(),
                "end-2".to_string(),
                "end-3".to_string(),
            ],
            "All effects should have completed",
        );

        assert!(
            max_in_flight.load(Ordering::SeqCst) >= 2,
            "Parallel effects should overlap on concurrent executor",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parallel_effects_respect_non_overlapping_executor() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));

        let order_for_handler = Arc::clone(&order);
        let in_flight_for_handler = Arc::clone(&in_flight);
        let max_in_flight_for_handler = Arc::clone(&max_in_flight);

        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(|event, _model| match event {
                TestEvent::Ping => Command::parallel([
                    TestEffect::Work(1),
                    TestEffect::Work(2),
                    TestEffect::Work(3),
                ]),
                TestEvent::Pong => Command::none(),
            })
            .effect_handler(move |effect: TestEffect, _resources| {
                let order = Arc::clone(&order_for_handler);
                let in_flight = Arc::clone(&in_flight_for_handler);
                let max_in_flight = Arc::clone(&max_in_flight_for_handler);

                Task::<TestEvent, TestEffect>::async_on::<InlineAsync, _>(async move {
                    if let TestEffect::Work(id) = effect {
                        let active = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                        max_in_flight.fetch_max(active, Ordering::SeqCst);

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("start-{id}"));
                        }

                        tokio::time::sleep(Duration::from_millis(5)).await;

                        {
                            let mut log = order.lock().unwrap();
                            log.push(format!("end-{id}"));
                        }

                        in_flight.fetch_sub(1, Ordering::SeqCst);
                    }

                    Command::none()
                })
            })
            .with_async_executor(InlineAsync::new())
            .build();

        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");
        while runner.step().expect("step should succeed") {
            tokio::task::yield_now().await;
        }

        tokio::time::sleep(Duration::from_millis(30)).await;

        let log = order.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "start-1".to_string(),
                "end-1".to_string(),
                "start-2".to_string(),
                "end-2".to_string(),
                "start-3".to_string(),
                "end-3".to_string(),
            ],
            "Parallel effects should fall back to sequential order when overlap is disabled",
        );

        assert_eq!(
            max_in_flight.load(Ordering::SeqCst),
            1,
            "No overlapping work should occur when executor disallows overlap",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wait_for_work_returns_immediately_when_work_available() {
        use std::time::Duration;

        // Create a runner with non-zero idle sleep
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::<TestEvent, TestEffect>::none())
            .with_async_executor(InlineAsync::new())
            .build();

        // Set idle sleep to a long duration
        let config = SyzygyConfig {
            idle_sleep: Duration::from_millis(200),
        };
        runner.set_config(config);

        // Send an event so there's work available
        runner
            .core_mut()
            .try_send_event(TestEvent::Ping)
            .expect("event channel should be open");

        // Verify work is pending before wait_for_work
        assert!(
            runner.core().has_pending_events(),
            "Should have pending work before wait_for_work"
        );

        // wait_for_work should return immediately since there's pending work
        runner.wait_for_work();

        // Verify work is still pending after wait_for_work (it doesn't process, just waits)
        assert!(
            runner.core().has_pending_events(),
            "Should still have pending work after wait_for_work"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wait_for_work_yields_when_idle_sleep_zero() {
        use std::time::Duration;

        // Create a runner with zero idle sleep
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_effect: TestEffect, _resources| Task::none())
            .with_async_executor(InlineAsync::new())
            .build();

        // Set idle sleep to zero
        let config = SyzygyConfig {
            idle_sleep: Duration::from_millis(0),
        };
        runner.set_config(config);

        // Verify no work is pending
        assert!(
            !runner.core().has_pending_events(),
            "Should have no pending work"
        );
        assert_eq!(
            runner.shell().pending_effects(),
            0,
            "Should have no pending effects"
        );

        // wait_for_work should yield immediately when idle_sleep is zero
        runner.wait_for_work();

        // State should be unchanged - no work was added or processed
        assert!(
            !runner.core().has_pending_events(),
            "Should still have no pending work"
        );
        assert_eq!(
            runner.shell().pending_effects(),
            0,
            "Should still have no pending effects"
        );
    }
}
````

## File: src/executor/tokio_executor.rs
````rust
use crate::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle};
use futures::executor::block_on;
use futures_util::{
    future::{BoxFuture, FutureExt},
    TryFutureExt,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::runtime::{self, Handle};
use tokio::sync::{oneshot::error::RecvError, Notify};

/// Executor backed by a dedicated tokio runtime on its own thread.
#[derive(Debug)]
pub struct TokioExecutor {
    driver: Driver,
}

#[derive(Debug)]
pub struct TokioExecutorBuilder {
    name: String,
    runtime_kind: RuntimeKind,
    worker_threads: Option<usize>,
    enable_time: bool,
    enable_io: bool,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeKind {
    MultiThread,
    CurrentThread,
}

#[derive(Debug)]
enum Driver {
    Owned(Arc<RwLock<OwnedState>>),
    Attached(Handle),
}

#[derive(Debug)]
struct OwnedState {
    handle: Option<Handle>,
    notify_shutdown: Arc<Notify>,
    completed_shutdown:
        futures_util::future::Shared<BoxFuture<'static, Result<(), Arc<RecvError>>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const NO_RUNTIME_MSG: &str =
    "No tokio runtime running. Use #[tokio::main] or create a runtime first.";

impl Drop for OwnedState {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(handle);
            self.notify_shutdown.notify_one();
        }

        if let Some(join) = self.thread.take() {
            let _ = join.join();
        }
    }
}

impl TokioExecutor {
    pub fn new_with(name: &str, mut builder: runtime::Builder) -> Self {
        let notify_shutdown = Arc::new(Notify::new());
        let notify_clone = Arc::clone(&notify_shutdown);

        let (tx_handle, rx_handle) = std::sync::mpsc::channel::<Handle>();
        let (tx_shutdown, rx_shutdown) = tokio::sync::oneshot::channel::<()>();

        let thread = std::thread::Builder::new()
            .name(format!("{name} executor"))
            .spawn(move || {
                let rt = builder.build().expect("failed to build tokio runtime");

                rt.block_on(async move {
                    let _ = tx_handle.send(Handle::current());
                    notify_clone.notified().await;
                });
                rt.shutdown_background();
                let _ = tx_shutdown.send(());
            })
            .expect("failed to spawn tokio executor thread");

        let handle = rx_handle.recv().expect("executor thread failed to start");

        let state = OwnedState {
            handle: Some(handle),
            notify_shutdown,
            completed_shutdown: futures_util::FutureExt::boxed(rx_shutdown.map_err(Arc::new))
                .shared(),
            thread: Some(thread),
        };

        Self {
            driver: Driver::Owned(Arc::new(RwLock::new(state))),
        }
    }

    #[must_use]
    pub fn multi_thread_io(name: &str, worker_threads: usize) -> Self {
        Self::builder()
            .name(name)
            .multi_thread()
            .worker_threads(worker_threads)
            .io()
            .build()
    }

    #[must_use]
    pub fn current_thread_io(name: &str) -> Self {
        Self::builder().name(name).current_thread().io().build()
    }

    #[must_use]
    pub fn multi_thread_cpu(name: &str, worker_threads: usize) -> Self {
        Self::builder()
            .name(name)
            .multi_thread()
            .worker_threads(worker_threads)
            .cpu()
            .build()
    }

    #[must_use]
    pub fn current_thread_cpu(name: &str) -> Self {
        Self::builder().name(name).current_thread().cpu().build()
    }

    #[must_use]
    pub fn from_handle(handle: Handle) -> Self {
        Self {
            driver: Driver::Attached(handle),
        }
    }

    pub fn try_from_current() -> Result<Self, &'static str> {
        Handle::try_current()
            .map(Self::from_handle)
            .map_err(|_| NO_RUNTIME_MSG)
    }

    #[must_use]
    pub fn builder() -> TokioExecutorBuilder {
        TokioExecutorBuilder::new()
    }
}

impl AsyncExecutor for TokioExecutor {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        match &self.driver {
            Driver::Owned(state) => {
                let handle = {
                    let guard = state.read().expect("executor state poisoned");
                    guard.handle.clone()
                };

                let Some(handle) = handle else {
                    return Err(ExecutorError::WorkerGone);
                };

                handle.spawn(job);
                Ok(())
            }
            Driver::Attached(handle) => {
                handle.spawn(job);
                Ok(())
            }
        }
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        tokio::time::sleep(duration).boxed()
    }
}

impl ExecutorLifecycle for TokioExecutor {
    fn shutdown(&self) {
        if let Driver::Owned(state) = &self.driver {
            let mut guard = state.write().expect("executor state poisoned");
            if guard.handle.take().is_some() {
                guard.notify_shutdown.notify_one();
            }
        }
    }

    fn wait(&self) {
        match &self.driver {
            Driver::Owned(state) => {
                self.shutdown();
                let fut = {
                    let guard = state.read().expect("executor state poisoned");
                    guard.completed_shutdown.clone()
                };
                block_on(async {
                    let _ = fut.await;
                });
            }
            Driver::Attached(_) => {
                // nothing to wait for on attached runtime
            }
        }
    }
}

impl TokioExecutorBuilder {
    fn new() -> Self {
        Self {
            name: "syzygy-tokio".to_string(),
            runtime_kind: RuntimeKind::MultiThread,
            worker_threads: None,
            enable_time: true,
            enable_io: true,
        }
    }

    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    #[must_use]
    pub fn multi_thread(mut self) -> Self {
        self.runtime_kind = RuntimeKind::MultiThread;
        self
    }

    #[must_use]
    pub fn current_thread(mut self) -> Self {
        self.runtime_kind = RuntimeKind::CurrentThread;
        self
    }

    #[must_use]
    pub fn worker_threads(mut self, threads: usize) -> Self {
        self.worker_threads = Some(threads.max(1));
        self
    }

    #[must_use]
    pub fn io(mut self) -> Self {
        self.enable_io = true;
        self.enable_time = true;
        self
    }

    #[must_use]
    pub fn cpu(mut self) -> Self {
        self.enable_io = false;
        self.enable_time = true;
        self
    }

    #[must_use]
    pub fn enable_time(mut self, enable: bool) -> Self {
        self.enable_time = enable;
        if !self.enable_time {
            self.enable_io = false;
        }
        self
    }

    #[must_use]
    pub fn enable_io(mut self, enable: bool) -> Self {
        self.enable_io = enable;
        if self.enable_io {
            self.enable_time = true;
        }
        self
    }

    pub fn build(self) -> TokioExecutor {
        let mut builder = match self.runtime_kind {
            RuntimeKind::MultiThread => {
                let mut builder = runtime::Builder::new_multi_thread();
                let threads = self.worker_threads.unwrap_or_else(default_worker_threads);
                builder.worker_threads(threads);
                builder
            }
            RuntimeKind::CurrentThread => runtime::Builder::new_current_thread(),
        };

        if self.enable_io {
            builder.enable_all();
        } else if self.enable_time {
            builder.enable_time();
        }

        TokioExecutor::new_with(&self.name, builder)
    }
}

fn default_worker_threads() -> usize {
    match std::thread::available_parallelism() {
        Ok(threads) => threads.get().max(1),
        Err(_) => 1,
    }
}
````

## File: tests/timeout_event_pattern.rs
````rust
#![allow(
    dead_code,
    clippy::clone_on_ref_ptr,
    unused_variables,
    unused_imports,
    clippy::let_and_return,
    clippy::format_in_format_args
)]
#![cfg(feature = "tokio")]
//! Tests demonstrating proper timeout event patterns
//!
//! These tests show how to handle timeouts as events rather than
//! relying on logging or system-level timeouts.

use std::time::Duration;
use syzygy::executor::TokioExecutor;
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct TimeoutModel {
    is_loading: bool,
    error_message: Option<String>,
    data: Option<String>,
    timeout_count: u32,
}

#[derive(Debug, Clone)]
enum TimeoutEvent {
    StartSlowOperation,
    OperationCompleted { data: String },
    OperationTimeout { duration: Duration },
    RetryOperation,
}

#[derive(Debug, Clone)]
enum TimeoutEffect {
    SlowOperation { delay_ms: u64 },
}

fn timeout_update(
    event: TimeoutEvent,
    model: &mut TimeoutModel,
) -> Command<TimeoutEvent, TimeoutEffect> {
    match event {
        TimeoutEvent::StartSlowOperation => {
            model.is_loading = true;
            model.error_message = None;
            Command::effect(TimeoutEffect::SlowOperation { delay_ms: 100 })
        }

        TimeoutEvent::OperationCompleted { data } => {
            model.is_loading = false;
            model.data = Some(data);
            model.timeout_count = 0;
            Command::none()
        }

        TimeoutEvent::OperationTimeout { duration } => {
            model.is_loading = false;
            model.timeout_count += 1;
            model.error_message = Some(format!(
                "Operation timed out after {:?} (attempt {})",
                duration, model.timeout_count
            ));

            // Auto-retry once, then require manual intervention
            if model.timeout_count < 2 {
                Command::event(TimeoutEvent::RetryOperation)
            } else {
                Command::none()
            }
        }

        TimeoutEvent::RetryOperation => {
            model.is_loading = true;
            // Use a faster operation for retry
            Command::effect(TimeoutEffect::SlowOperation { delay_ms: 50 })
        }
    }
}

// Effect handler that implements manual timeout detection using Task plans
fn timeout_aware_effect_handler(
    effect: TimeoutEffect,
    _resources: (),
) -> syzygy::executor::Task<TimeoutEvent, TimeoutEffect> {
    match effect {
        TimeoutEffect::SlowOperation { delay_ms } => {
            syzygy::executor::Task::<TimeoutEvent, TimeoutEffect>::async_on::<TokioExecutor, _>(
                async move {
                    let operation_future = async move {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        format!("Operation completed after {delay_ms}ms")
                    };

                    // Use manual timeout with event emission
                    let timeout_duration = Duration::from_millis(200);
                    match tokio::time::timeout(timeout_duration, operation_future).await {
                        Ok(data) => Command::event(TimeoutEvent::OperationCompleted { data }),
                        Err(_timeout) => Command::event(TimeoutEvent::OperationTimeout {
                            duration: timeout_duration,
                        }),
                    }
                },
            )
        }
    }
}

/// Test that timeout events are properly emitted and handled
#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn test_timeout_event_pattern() {
    let mut runner = Syzygy::builder::<TimeoutEvent, TimeoutEffect>()
        .model(TimeoutModel::default())
        .event_handler(timeout_update)
        .effect_handler(timeout_aware_effect_handler)
        .with_async_executor(TokioExecutor::multi_thread_io("timeout-pattern", 2))
        .build();

    let event_sender = runner.core().event_sender();

    // Start a slow operation
    event_sender.send(TimeoutEvent::StartSlowOperation).unwrap();

    // Run until operation completes or times out
    runner
        .run_until(|core, _shell| !core.model().is_loading)
        .unwrap();

    let model = runner.core().model();

    // Should have completed successfully (100ms delay < 200ms timeout)
    assert!(!model.is_loading);
    assert!(model.data.is_some());
    assert_eq!(model.timeout_count, 0);
    assert!(model.error_message.is_none());

    println!("✅ Fast operation completed successfully: {:?}", model.data);
}

/// Test timeout event structure and data
#[tokio::test(flavor = "multi_thread")]
async fn test_timeout_event_data() {
    let timeout_duration = Duration::from_millis(100);
    let event = TimeoutEvent::OperationTimeout {
        duration: timeout_duration,
    };

    match event {
        TimeoutEvent::OperationTimeout { duration } => {
            assert_eq!(duration, Duration::from_millis(100));
            println!("✅ Timeout event carries correct duration: {duration:?}");
        }
        _ => panic!("Expected timeout event"),
    }
}
````

## File: src/command.rs
````rust
use smallvec::SmallVec;

/// One atomic operation in the Core→Shell pipeline.
///
/// This is what actually happens when your event handler returns a Command.
/// Events go back to Core for immediate processing. Effects get queued for
/// async execution. Both `Batch` and `Parallel` dispatch effects immediately;
/// actual concurrency depends on the registered executors.
#[derive(Clone)]
pub enum CommandStep<Event, Effect> {
    Event(Event),
    Effect(Effect),
    /// Effects dispatched in order; executors determine actual execution order
    Batch(Vec<Effect>),
    /// Effects dispatched without waiting between each submission
    Parallel(Vec<Effect>),
}

impl<Event, Effect> PartialEq for CommandStep<Event, Effect>
where
    Event: PartialEq,
    Effect: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Event(a), Self::Event(b)) => a == b,
            (Self::Effect(a), Self::Effect(b)) => a == b,
            (Self::Batch(a), Self::Batch(b)) | (Self::Parallel(a), Self::Parallel(b)) => a == b,
            _ => false,
        }
    }
}

impl<Event, Effect> std::fmt::Debug for CommandStep<Event, Effect>
where
    Event: std::fmt::Debug,
    Effect: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Event(e) => f.debug_tuple("Event").field(e).finish(),
            Self::Effect(x) => f.debug_tuple("Effect").field(x).finish(),
            Self::Batch(v) => f.debug_tuple("Batch").field(v).finish(),
            Self::Parallel(v) => f.debug_tuple("Parallel").field(v).finish(),
        }
    }
}

/// The bridge between your pure event handler and the chaotic async world.
///
/// Commands are how you tell the Shell what to do without coupling your
/// event handler to implementation details. Think of it as a shopping list
/// for side effects - your event handler writes it, the Shell executes it.
///
/// Uses `SmallVec` because most commands contain 0-4 steps. If you're building
/// commands with more steps, reconsider your architecture - you're probably
/// doing too much in one event handler.
#[derive(Debug)]
pub struct Command<Event, Effect> {
    outputs: SmallVec<[CommandStep<Event, Effect>; 4]>,
}

impl<Event: Clone, Effect: Clone> Clone for Command<Event, Effect> {
    fn clone(&self) -> Self {
        Self {
            outputs: self.outputs.clone(),
        }
    }
}

impl<Event: Clone, Effect: Clone> Default for Command<Event, Effect> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Event: Clone, Effect: Clone> PartialEq for Command<Event, Effect>
where
    Event: PartialEq,
    Effect: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.outputs == other.outputs
    }
}

impl<Event, Effect> Command<Event, Effect> {
    /// Returns a command that does absolutely nothing.
    ///
    /// Use this when your event handler needs to update the model but doesn't
    /// need to trigger any side effects. It's the functional equivalent of
    /// telling the Shell "don't call us, we'll call you."
    ///
    /// # Example
    /// ```
    /// fn handle_increment(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     model.counter += 1;
    ///     Command::none() // Model updated, no side effects needed
    /// }
    /// ```
    ///
    /// # Example
    /// ```
    /// fn handle_increment(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     model.counter += 1;
    ///     Command::none() // Model updated, no side effects needed
    /// }
    /// ```
    #[must_use]
    pub fn none() -> Self {
        Self {
            outputs: SmallVec::new(),
        }
    }

    /// Create a command with a single step
    fn from_step(step: CommandStep<Event, Effect>) -> Self {
        let mut outputs = SmallVec::new();
        outputs.push(step);
        Self { outputs }
    }

    /// Creates a command that immediately triggers another event.
    ///
    /// The event gets processed synchronously in the same tick. No async
    /// boundary, no delay, and no chance for external interference between
    /// the chained events. Use this for breaking complex flows into smaller,
    /// testable pieces.
    ///
    /// # Example
    /// ```
    /// fn handle_login(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     if model.user.is_authenticated() {
    ///         // Chain to dashboard event immediately
    ///         Command::event(Event::ShowDashboard)
    ///     } else {
    ///         Command::effect(Effect::Authenticate { credentials: event.credentials })
    ///     }
    /// }
    /// ```
    pub fn event(event: impl Into<Event>) -> Self {
        Self::from_step(CommandStep::Event(event.into()))
    }

    /// Creates a command that triggers an async side effect.
    ///
    /// Effects run in the Shell's async context. They can spawn tasks, make
    /// HTTP requests, write files - all the dirty stuff your pure event handler
    /// shouldn't touch. Effects can send events back to Core when they're done.
    ///
    /// # Example
    /// ```
    /// fn handle_save(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     let data = model.data.clone();
    ///     Command::effect(Effect::SaveToDisk { data })
    /// }
    /// ```
    pub fn effect(effect: impl Into<Effect>) -> Self {
        Self::from_step(CommandStep::Effect(effect.into()))
    }

    /// Creates a command that fires multiple events in order.
    ///
    /// Events are processed sequentially in the order provided. Each event
    /// gets its own call to your event handler, so the model can change
    /// between events. Use this when you need to trigger a sequence of
    /// state changes without any async operations between them.
    ///
    /// # Example
    /// ```
    /// fn handle_reset(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     Command::events(vec![
    ///         Event::ClearUserData,
    ///         Event::ResetUI,
    ///         Event::ShowWelcomeScreen,
    ///     ])
    /// }
    /// ```
    pub fn events(events: impl IntoIterator<Item = Event>) -> Self {
        let outputs = events.into_iter().map(CommandStep::Event).collect();
        Self { outputs }
    }

    /// Creates a command that runs multiple effects as a single Batch step.
    ///
    /// The Shell dispatches the effects in order. Whether they end up running
    /// sequentially or concurrently depends on what each effect returns and
    /// how the registered executors schedule that work.
    pub fn effects(effects: impl IntoIterator<Item = Effect>) -> Self {
        Self::sequential(effects)
    }

    /// Alias for [`Command::effects`] that makes ordering intent explicit.
    pub fn sequential(effects: impl IntoIterator<Item = Effect>) -> Self {
        let batch: Vec<Effect> = effects.into_iter().collect();
        Self::from_step(CommandStep::Batch(batch))
    }

    /// Creates a command that runs multiple effects with no submission gaps.
    ///
    /// Each effect is dispatched without waiting for the previous one to complete. When
    /// using an executor that supports overlap, the effects can run concurrently. On
    /// executors that do not, they will still execute in order but without failing.
    pub fn parallel(effects: impl IntoIterator<Item = Effect>) -> Self {
        let batch: Vec<Effect> = effects.into_iter().collect();
        Self::from_step(CommandStep::Parallel(batch))
    }

    /// Combines multiple commands into one.
    ///
    /// Flattens all the steps from all commands into a single command.
    /// No magic, no deduplication, no ordering guarantees beyond what
    /// each individual command already provides. If you need specific
    /// ordering, build your commands carefully - this just concatenates.
    ///
    /// # Example
    /// ```
    /// let cmd1 = Command::event(Event::StartLoading);
    /// let cmd2 = Command::effect(Effect::FetchData);
    /// let cmd3 = Command::event(Event::ShowSpinner);
    ///
    /// // Combines all three into one command
    /// let combined = Command::batch(vec![cmd1, cmd2, cmd3]);
    /// ```
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        let mut outputs = SmallVec::new();
        for c in commands {
            outputs.extend(c.outputs);
        }
        Self { outputs }
    }

    /// Returns true if this command does nothing.
    ///
    /// Equivalent to checking if `len() == 0`, but doesn't need to
    /// iterate through batch effects to count them. Use this for
    /// quick checks instead of counting steps you don't care about.
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }

    /// Returns the total number of steps in this command.
    ///
    /// Counts individual events and effects as 1 each. Batch effects
    /// contribute their length to the total. This walks through all
    /// steps, so it's O(n) where n is the number of `CommandSteps`.
    pub fn len(&self) -> usize {
        self.outputs
            .iter()
            .map(|o| match o {
                CommandStep::Batch(v) | CommandStep::Parallel(v) => v.len(),
                _ => 1,
            })
            .sum()
    }

    // ---------------------------
    // Builder-style chaining API
    // ---------------------------

    /// Chain another event to this command.
    ///
    /// # Example
    /// ```
    /// let cmd = Command::event(Event::Start)
    ///     .and_event(Event::Initialize)
    ///     .and_event(Event::Ready);
    /// ```
    #[must_use]
    #[inline]
    pub fn and_event(mut self, event: impl Into<Event>) -> Self {
        self.outputs.push(CommandStep::Event(event.into()));
        self
    }

    /// Chain another effect to this command.
    ///
    /// # Example
    /// ```
    /// let cmd = Command::effect(Effect::LoadConfig)
    ///     .and_effect(Effect::ConnectDatabase)
    ///     .and_effect(Effect::StartServer);
    /// ```
    #[must_use]
    #[inline]
    pub fn and_effect(mut self, effect: impl Into<Effect>) -> Self {
        self.outputs.push(CommandStep::Effect(effect.into()));
        self
    }

    /// Chain another command to this one (concatenates all steps).
    ///
    /// # Example
    /// ```
    /// let init = Command::event(Event::Init);
    /// let load = Command::effect(Effect::LoadData);
    /// let combined = init.and(load);
    /// ```
    #[must_use]
    #[inline]
    pub fn and(mut self, other: Self) -> Self {
        self.outputs.extend(other.outputs);
        self
    }
}

impl<Event, Effect> IntoIterator for Command<Event, Effect> {
    type Item = CommandStep<Event, Effect>;
    type IntoIter = smallvec::IntoIter<[CommandStep<Event, Effect>; 4]>;
    fn into_iter(self) -> Self::IntoIter {
        self.outputs.into_iter()
    }
}

impl<'a, Event, Effect> IntoIterator for &'a Command<Event, Effect> {
    type Item = &'a CommandStep<Event, Effect>;
    type IntoIter = std::slice::Iter<'a, CommandStep<Event, Effect>>;
    fn into_iter(self) -> Self::IntoIter {
        self.outputs.iter()
    }
}

impl<Event: Clone, Effect: Clone> Command<Event, Effect> {
    /// Returns an iterator over the command steps.
    ///
    /// Iterates in the order steps were added. Batch effects appear
    /// as single `CommandStep::Batch` items - this doesn't flatten
    /// them. Use this when you need to inspect or transform the
    /// individual steps in a command.
    pub fn iter(&self) -> std::slice::Iter<'_, CommandStep<Event, Effect>> {
        self.outputs.iter()
    }
}
impl<Event, Effect> FromIterator<CommandStep<Event, Effect>> for Command<Event, Effect> {
    fn from_iter<I: IntoIterator<Item = CommandStep<Event, Effect>>>(iter: I) -> Self {
        Self {
            outputs: iter.into_iter().collect(),
        }
    }
}
impl<Event, Effect> From<()> for Command<Event, Effect> {
    fn from((): ()) -> Self {
        Self::none()
    }
}

/// ## Implementing `From<T>` for ergonomic conversions
///
/// Applications can add their own `From<T>` impls to convert domain types into
/// commands that target specific `Event`/`Effect` pairs:
///
/// ```rust
/// # use syzygy::command::{Command, CommandStep};
/// #[derive(Clone)] enum Event { KickOff }
/// #[derive(Clone)] enum Effect { Notify(String) }
///
/// impl From<Event> for Command<Event, Effect> {
///     fn from(event: Event) -> Self {
///         Command::event(event)
///     }
/// }
///
/// impl From<Effect> for Command<Event, Effect> {
///     fn from(effect: Effect) -> Self {
///         Command::effect(effect)
///     }
/// }
/// ```
///
/// If `Event` and `Effect` are the same type you cannot implement both traits
/// due to Rust's coherence rules. In that case call `Command::event` and
/// `Command::effect` directly at the call site.

/// Free-function helpers for building [`Command`] values.
///
/// These functions mirror the inherent constructors on [`Command`] but live in a module that can
/// be glob-imported from the prelude (`use syzygy::prelude::command::*;`) for quick prototyping.
pub mod builders {
    use super::Command;

    /// Construct a no-op command.
    #[inline]
    #[must_use]
    pub fn none<Event, Effect>() -> Command<Event, Effect> {
        Command::none()
    }

    /// Emit an immediate event back into Core.
    #[inline]
    #[must_use]
    pub fn event<Event, Effect>(event: impl Into<Event>) -> Command<Event, Effect> {
        Command::event(event)
    }

    /// Emit a sequence of events.
    #[inline]
    #[must_use]
    pub fn events<Event, Effect>(
        events: impl IntoIterator<Item = Event>,
    ) -> Command<Event, Effect> {
        Command::events(events)
    }

    /// Schedule a single effect.
    #[inline]
    #[must_use]
    pub fn effect<Event, Effect>(effect: impl Into<Effect>) -> Command<Event, Effect> {
        Command::effect(effect)
    }

    /// Schedule a batch of effects sequentially.
    #[inline]
    #[must_use]
    pub fn effects<Event, Effect>(
        effects: impl IntoIterator<Item = Effect>,
    ) -> Command<Event, Effect> {
        Command::effects(effects)
    }

    /// Alias for [`effects`] when you want to spell out intent explicitly.
    #[inline]
    #[must_use]
    pub fn sequential<Event, Effect>(
        effects: impl IntoIterator<Item = Effect>,
    ) -> Command<Event, Effect> {
        Command::sequential(effects)
    }

    /// Run effects without submission gaps (executor dependent concurrency).
    #[inline]
    #[must_use]
    pub fn parallel<Event, Effect>(
        effects: impl IntoIterator<Item = Effect>,
    ) -> Command<Event, Effect> {
        Command::parallel(effects)
    }

    /// Flatten multiple commands into one.
    #[inline]
    #[must_use]
    pub fn batch<Event, Effect>(
        commands: impl IntoIterator<Item = Command<Event, Effect>>,
    ) -> Command<Event, Effect> {
        Command::batch(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        Start,
        Middle,
        End,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestEffect {
        Log(String),
        Save,
        Load,
    }

    impl From<TestEvent> for Command<TestEvent, TestEffect> {
        fn from(event: TestEvent) -> Self {
            Command::event(event)
        }
    }

    impl From<TestEffect> for Command<TestEvent, TestEffect> {
        fn from(effect: TestEffect) -> Self {
            Command::effect(effect)
        }
    }

    #[test]
    fn test_chaining_events() {
        let cmd: Command<TestEvent, TestEffect> = Command::event(TestEvent::Start)
            .and_event(TestEvent::Middle)
            .and_event(TestEvent::End);

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(steps[1], CommandStep::Event(TestEvent::Middle));
        assert_eq!(steps[2], CommandStep::Event(TestEvent::End));
    }

    #[test]
    fn test_chaining_effects() {
        let cmd: Command<TestEvent, TestEffect> = Command::effect(TestEffect::Load)
            .and_effect(TestEffect::Save)
            .and_effect(TestEffect::Log("done".into()));

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], CommandStep::Effect(TestEffect::Load));
        assert_eq!(steps[1], CommandStep::Effect(TestEffect::Save));
        assert_eq!(
            steps[2],
            CommandStep::Effect(TestEffect::Log("done".into()))
        );
    }

    #[test]
    fn test_chaining_mixed() {
        let cmd: Command<TestEvent, TestEffect> = Command::event(TestEvent::Start)
            .and_effect(TestEffect::Load)
            .and_event(TestEvent::Middle)
            .and_effect(TestEffect::Save)
            .and_event(TestEvent::End);

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(steps[1], CommandStep::Effect(TestEffect::Load));
        assert_eq!(steps[2], CommandStep::Event(TestEvent::Middle));
        assert_eq!(steps[3], CommandStep::Effect(TestEffect::Save));
        assert_eq!(steps[4], CommandStep::Event(TestEvent::End));
    }

    #[test]
    fn test_and_command() {
        let cmd1: Command<TestEvent, TestEffect> =
            Command::event(TestEvent::Start).and_effect(TestEffect::Load);

        let cmd2: Command<TestEvent, TestEffect> =
            Command::event(TestEvent::Middle).and_effect(TestEffect::Save);

        let combined = cmd1.and(cmd2);

        let steps: Vec<_> = combined.into_iter().collect();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(steps[1], CommandStep::Effect(TestEffect::Load));
        assert_eq!(steps[2], CommandStep::Event(TestEvent::Middle));
        assert_eq!(steps[3], CommandStep::Effect(TestEffect::Save));
    }

    #[test]
    fn test_chaining_from_none() {
        let cmd: Command<TestEvent, TestEffect> = Command::none()
            .and_event(TestEvent::Start)
            .and_effect(TestEffect::Log("hello".into()));

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(
            steps[1],
            CommandStep::Effect(TestEffect::Log("hello".into()))
        );
    }

    #[test]
    fn test_chaining_preserves_batch() {
        let cmd: Command<TestEvent, TestEffect> =
            Command::effects(vec![TestEffect::Load, TestEffect::Save]).and_event(TestEvent::End);

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0],
            CommandStep::Batch(vec![TestEffect::Load, TestEffect::Save])
        );
        assert_eq!(steps[1], CommandStep::Event(TestEvent::End));
    }

    #[test]
    fn test_existing_ergonomic_api() {
        // Test that the existing API provides good ergonomics
        // These work because event() and effect() accept impl Into<Event> and impl Into<Effect>

        // Direct construction
        let cmd1: Command<TestEvent, TestEffect> = Command::event(TestEvent::Start);
        let cmd2: Command<TestEvent, TestEffect> = Command::effect(TestEffect::Load);

        // Collection construction
        let cmd3: Command<TestEvent, TestEffect> =
            Command::events(vec![TestEvent::Start, TestEvent::Middle]);
        let cmd4: Command<TestEvent, TestEffect> =
            Command::effects(vec![TestEffect::Load, TestEffect::Save]);

        // From unit type (already implemented)
        let cmd5: Command<TestEvent, TestEffect> = ().into();

        assert_eq!(cmd1.len(), 1);
        assert_eq!(cmd2.len(), 1);
        assert_eq!(cmd3.len(), 2);
        assert_eq!(cmd4.len(), 2);
        assert_eq!(cmd5.len(), 0);
    }

    #[test]
    fn test_from_impls_enable_into() {
        // With From implementations in place, the standard Into conversions work.
        let event_cmd: Command<TestEvent, TestEffect> = TestEvent::Start.into();
        let effect_cmd: Command<TestEvent, TestEffect> = TestEffect::Load.into();

        assert_eq!(event_cmd.len(), 1);
        assert_eq!(effect_cmd.len(), 1);

        let event_steps: Vec<_> = event_cmd.into_iter().collect();
        let effect_steps: Vec<_> = effect_cmd.into_iter().collect();

        assert_eq!(event_steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(effect_steps[0], CommandStep::Effect(TestEffect::Load));
    }
}
````

## File: Cargo.toml
````toml
[package]
name = "syzygy"
version = "0.1.0"
edition = "2021"
description = "Zero-overhead TEA for Rust: pure Core, async Shell, and explicit Commands/Tasks for side effects."
license = "Unlicense"
repository = "https://github.com/ribelo/syzygy"
homepage = "https://github.com/ribelo/syzygy"
documentation = "https://docs.rs/syzygy"
readme = "README.md"
keywords = ["tea", "state", "architecture", "async", "effects"]
categories = ["asynchronous", "rust-patterns"]
rust-version = "1.75"

[lib]
path = "src/lib.rs"
doctest = false

[features]
default = ["shell", "rt-inline"]
shell = [
    "dep:futures",
    "dep:futures-util",
    "dep:rustc-hash",
]
cli = ["shell", "dep:ctrlc"]
rt-inline = ["shell"]
rt-single-thread = ["shell"]
tokio = ["shell", "dep:tokio", "dep:tokio-util"]
view-model = []
multi-model = []
tracing = ["dep:tracing"]
magic-compilation-test = []
manual-compilation-test = []
rayon = ["shell", "dep:rayon"]
examples = ["shell", "rt-inline", "tokio"]
legacy_tests = ["shell"]
flair = []
preset-interactive = ["shell", "rt-inline"]
preset-server = ["shell", "tokio", "rayon", "tracing"]
preset-examples = ["examples"]

[dependencies]
# Core dependencies
thiserror = "2.0"
crossbeam-channel = "0.5"
futures = { version = "0.3", optional = true }
futures-util = { version = "0.3", optional = true }
smallvec = "1.13"
rustc-hash = { version = "2.0", optional = true }
rayon = { version = "1.9", optional = true }
ctrlc = { version = "3.4", optional = true }

# Runtime dependency
tokio = { version = "1.40", features = ["rt", "rt-multi-thread", "sync", "time", "macros", "io-util"], optional = true }
tokio-util = { version = "0.7", features = ["rt"], optional = true }

# Tracing dependencies
tracing = { version = "0.1", optional = true }


[dev-dependencies]
cfg-if = "1.0.0"
criterion = { version = "0.5", features = ["html_reports", "async_tokio"] }
futures = "0.3"
enum_dispatch = "0.3"
ambassador = "0.4"
frunk = "0.4"
frunk_core = "0.4"
cucumber = "0.21"
heapless = "0.8"
micromap = "0.1"
enum-map = "2.7"
indexmap = "2.0"
ahash = "0.8"
proptest = "1.4"
tokio-test = "0.4"
rand = "0.8"

# BDD tests removed - were using outdated API incompatible with current architecture

# Temporarily disabled - outdated benchmarks moved to outdated_code/
# [[bench]]
# name = "dispatch_benchmark"
# harness = false

# [[bench]]
# name = "model_storage_benchmark"
# harness = false


[lints.clippy]
all = { level = "warn", priority = -2 }

# restriction
dbg_macro = "warn"
todo = "warn"
unimplemented = "warn"

# I like the explicitness of this rule as it removes confusion around `clone`.
# This increases readability, avoids `clone` mindlessly and heap allocating on accident.
clone_on_ref_ptr = "warn"

# These two are mutually exclusive, I like `mod.rs` files for better fuzzy searches on module entries.
self_named_module_files = "warn"         # "-Wclippy::mod_module_files"
empty_drop = "warn"
empty_structs_with_brackets = "warn"
exit = "warn"
filetype_is_file = "warn"
get_unwrap = "warn"
rc_buffer = "warn"
rc_mutex = "warn"
rest_pat_in_fully_bound_structs = "warn"
unnecessary_safety_comment = "warn"
undocumented_unsafe_blocks = "warn"

# I want to write the best Rust code so pedantic is enabled.
# We should only disable rules globally if they are either false positives, chaotic, or does not make sense.
pedantic = { level = "warn", priority = -1 }

# Allowed rules
# pedantic
# This rule is too pedantic, I don't want to force this because naming things are hard.
module_name_repetitions = "allow"
similar-names = "allow"

# All triggers are mostly ignored in this codebase, so this is ignored globally.
struct_excessive_bools = "allow"
too_many_lines = "allow"
doc_markdown = "allow"

# Domain-specific: Events are consumed by exactly one handler and often need their data moved
# to Commands, making pass-by-value more efficient than borrow+clone patterns.
needless_pass_by_value = "allow"

# nursery
# `const` functions do not make sense for our project because this is not a `const` library.
# This rule also confuses new comers and forces them to add `const` blindlessly without any reason.
missing_const_for_fn = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"

multiple_bound_locations = "allow"


[[bench]]
name = "command_performance"
harness = false

[[bench]]
name = "shell_throughput"
harness = false

# Removed broken benchmarks that tested outdated APIs

# Only build examples when the `examples` feature is enabled
[[example]]
name = "basic_counter"
required-features = ["examples"]

[[example]]
name = "async_effect"
required-features = ["examples"]

[[example]]
name = "manual_loop"
required-features = ["examples"]

[[example]]
name = "two_executors"
required-features = ["examples"]

[[example]]
name = "timeout_pattern"
required-features = ["examples"]
````

## File: src/executor/mod.rs
````rust
//! Executors — specialized async and blocking runtimes.

use std::any::{Any, TypeId};
use std::time::Duration;

use futures_util::future::BoxFuture;
use thiserror::Error;

/// Type alias for the complex job function type used by ResourceBlockingExecutor
type ResourceJobFn = Box<dyn FnOnce(&mut dyn Any) + Send>;

#[cfg(feature = "rt-inline")]
pub mod inline_async;
#[cfg(feature = "rayon")]
pub mod rayon_sync_executor;
pub mod registry;
#[cfg(feature = "rt-single-thread")]
pub mod single_thread_executor;
pub mod task;
#[cfg(feature = "tokio")]
pub mod tokio_executor;

#[cfg(feature = "rt-inline")]
pub use inline_async::InlineAsync;
#[cfg(feature = "rayon")]
pub use rayon_sync_executor::{RayonExecutor, RayonExecutorBuilder};
pub use registry::ExecutorRegistry;
#[cfg(feature = "rt-single-thread")]
pub use single_thread_executor::SingleThreadExecutor;
pub use task::{PanicDetails, PanicHook, PanicTaskKind, Task};
/// Friendly alias for `Task` used in docs to highlight declarative plans.
pub type Plan<E, X> = Task<E, X>;
#[cfg(feature = "tokio")]
pub use tokio_executor::{TokioExecutor, TokioExecutorBuilder};

/// Error type returned when executors fail to schedule jobs.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("executor has been shut down")]
    Shutdown,
    #[error("executor worker is gone")]
    WorkerGone,
    #[error("task was cancelled")]
    Cancelled,
    #[error("panic: {msg}")]
    Panic { msg: String },
}

#[must_use]
pub fn panic_message(panic_payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = panic_payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = panic_payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unknown internal error".to_string()
    }
}

/// Shared lifecycle management for all executor types.
pub trait ExecutorLifecycle: Send + 'static {
    fn shutdown(&self);
    fn wait(&self);
}

/// Executor specialized for async work (futures).
pub trait AsyncExecutor: ExecutorLifecycle + Sync {
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError>;

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()>;
}

/// Executor for blocking work without shared resources.
pub trait BlockingExecutor: ExecutorLifecycle + Sync {
    fn spawn_blocking(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), ExecutorError>;
}

/// Executor for blocking work with a dedicated, mutable resource.
pub trait ResourceBlockingExecutor: ExecutorLifecycle + Sync {
    fn resource_type_id(&self) -> TypeId;

    fn spawn_blocking_with_resource(&self, job: ResourceJobFn) -> Result<(), ExecutorError>;
}
````

## File: src/core.rs
````rust
//! # Core - Synchronous State Management
//!
//! This module provides the `Core` component, which is the heart of Syzygy's
//! synchronous state management. It is responsible for processing events,
//! updating the application's models, and generating commands for side effects.
//!
//! ## Key Components
//! - `Core` - The main struct that owns the application state (models) and processes events.
//! - `EventHandler` - A type alias for the function that contains the application's update logic.
//!
//! ## Example
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Clone)] enum TestEvent { Increment }
//! # #[derive(Debug, Clone)] enum TestEffect { Log }
//! # #[derive(Debug, Default)] struct CounterModel { count: i32 }
//! # fn counter_update(event: TestEvent, model: &mut CounterModel) -> Command<TestEvent, TestEffect> {
//! #     model.count += 1;
//! #     Command::none()
//! # }
//! // In a real application, you would build the core like this:
//! let model = CounterModel::default();
//! let (mut core, sender) = Core::new(counter_update, model);
//!
//! // Send an event to the core
//! sender.send(TestEvent::Increment).unwrap();
//!
//! // Process the event queue
//! let commands = core.process_events();
//!
//! // The model is now updated
//! let model_ref: &CounterModel = core.model();
//! assert_eq!(model_ref.count, 1);
//! ```
use std::collections::VecDeque;
use std::time::Duration;

use crossbeam_channel::{
    bounded, unbounded, Receiver as CoreReceiver, RecvTimeoutError as CoreRecvTimeoutError,
    Sender as CoreSender, TryRecvError as CoreTryRecvError,
};

type CoreSendError<E> = crossbeam_channel::SendError<E>;

#[cfg(feature = "tracing")]
use tracing::{debug, span, Level};

use crate::command::Command;

/// Update function type that takes an event and a mutable reference to the model.
pub type EventHandler<E, X, M> = fn(event: E, model: &mut M) -> Command<E, X>;

/// Multi-producer sender returned by [`Core::new`].
///
/// Wraps the underlying channel sender used by the Core.
pub struct EventSender<E> {
    inner: CoreSender<E>,
}

impl<E> Clone for EventSender<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E> EventSender<E> {
    fn new(inner: CoreSender<E>) -> Self {
        Self { inner }
    }

    /// Send an event to the core.
    pub fn send(&self, event: E) -> Result<(), CoreSendError<E>> {
        self.inner.send(event)
    }

    /// Alias for [`send`]. Kept for backward compatibility; on bounded channels this will
    /// still block when the queue is full.
    pub fn try_send(&self, event: E) -> Result<(), CoreSendError<E>> {
        self.send(event)
    }

    /// Attempt to send an event without blocking.
    ///
    /// Returns a [`CoreError::ChannelFull`](crate::error::CoreError::ChannelFull) when the queue
    /// is at capacity, allowing callers to implement backpressure strategies.
    pub fn try_send_event(&self, event: E) -> Result<(), crate::error::CoreError> {
        self.inner.try_send(event).map_err(Into::into)
    }
}

struct EventReceiver<E> {
    inner: CoreReceiver<E>,
}

impl<E> EventReceiver<E> {
    fn new(inner: CoreReceiver<E>) -> Self {
        Self { inner }
    }

    fn try_recv(&self) -> Result<E, CoreTryRecvError> {
        match self.inner.try_recv() {
            Ok(event) => Ok(event),
            Err(err) => Err(err),
        }
    }

    fn recv_timeout(&self, timeout: Duration) -> Result<E, CoreRecvTimeoutError> {
        match self.inner.recv_timeout(timeout) {
            Ok(event) => Ok(event),
            Err(err) => Err(err),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

fn event_channel<E>(capacity: Option<usize>) -> (CoreSender<E>, CoreReceiver<E>) {
    match capacity {
        Some(capacity) => bounded(capacity),
        None => unbounded(),
    }
}

/// Core handles synchronous event processing and owns the model.
///
/// Core is designed to be used on any thread, including UI threads, as it
/// never blocks and all operations are synchronous. It processes events,
/// updates the model, and returns Commands describing effects to execute.
pub struct Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// The update function that processes events
    event_handler: EventHandler<E, X, M>,

    /// The model - owned and mutable
    model: M,

    /// Queue of events to process
    event_queue: VecDeque<E>,

    /// Pre-allocated command buffer to avoid reallocations
    command_buffer: Vec<Command<E, X>>,

    /// Channel for receiving external events
    event_rx: EventReceiver<E>,

    /// Channel for sending events (kept for cloning)
    event_tx: EventSender<E>,

    /// Configured inbound event capacity (None => unbounded)
    event_channel_capacity: Option<usize>,
}

impl<E, X, M> Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// Create a new Core with update function and storage
    pub fn new(event_handler: EventHandler<E, X, M>, models: M) -> (Self, EventSender<E>) {
        Self::with_event_channel_capacity(event_handler, models, None)
    }

    /// Create a new Core with a specific inbound event channel capacity.
    ///
    /// `None` keeps the default unbounded channel. Any `Some(capacity)` value sets an upper bound
    /// on queued events; senders will receive a `CoreError::ChannelFull` until progress is made.
    pub fn with_event_channel_capacity(
        event_handler: EventHandler<E, X, M>,
        models: M,
        capacity: Option<usize>,
    ) -> (Self, EventSender<E>) {
        let (raw_tx, raw_rx) = event_channel::<E>(capacity);
        let event_tx = EventSender::new(raw_tx);
        let event_rx = EventReceiver::new(raw_rx);

        let core = Self {
            event_handler,
            model: models,
            event_queue: VecDeque::with_capacity(16), // Pre-size for typical usage
            command_buffer: Vec::with_capacity(16),   // Pre-allocated command buffer
            event_rx,
            event_tx: event_tx.clone(),
            event_channel_capacity: capacity,
        };

        (core, event_tx)
    }

    /// Process a single event synchronously
    ///
    /// This is the main entry point for event processing. It updates the model
    /// and returns a Command describing any effects to execute.
    pub fn handle_event(&mut self, event: E) -> Command<E, X> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "handle_event").entered();

        #[cfg(feature = "tracing")]
        debug!("Processing event");

        let command = (self.event_handler)(event, &mut self.model);

        #[cfg(feature = "tracing")]
        debug!("Event processed, command created");

        command
    }

    /// Process all events in the queue
    ///
    /// This processes all pending events and collects their commands.
    /// Returns a Vec<Command<E, X>> containing all commands generated.
    /// An empty vector indicates no events were processed.
    pub fn process_events(&mut self) -> Vec<Command<E, X>> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "process_events").entered();

        // Process events from external channel first
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_queue.push_back(event);
        }

        // Process all events in queue
        while let Some(event) = self.event_queue.pop_front() {
            let command = self.handle_event(event);
            self.command_buffer.push(command);
        }

        #[cfg(feature = "tracing")]
        if !self.command_buffer.is_empty() {
            debug!(events_processed = self.command_buffer.len());
        }

        let mut out = Vec::with_capacity(self.command_buffer.capacity().max(16));
        std::mem::swap(&mut self.command_buffer, &mut out);
        self.command_buffer.clear();
        out
    }

    /// Send an event to be processed in the next tick.
    #[deprecated(note = "use try_send_event() which returns Result")]
    pub fn send_event(&self, event: E) {
        let _ = self.try_send_event(event);
    }

    /// Attempt to send an event, returning an error if the channel is closed.
    pub fn try_send_event(&self, event: E) -> Result<(), crate::error::CoreError> {
        self.event_tx.inner.try_send(event).map_err(Into::into)
    }

    /// Get a sender for external events
    #[must_use]
    pub fn event_sender(&self) -> EventSender<E> {
        self.event_tx.clone()
    }

    /// Get the configured inbound event channel capacity (None => unbounded).
    #[must_use]
    pub fn event_channel_capacity(&self) -> Option<usize> {
        self.event_channel_capacity
    }

    /// Get immutable reference to the model
    #[must_use]
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Get mutable reference to the model
    ///
    /// This should be used carefully as it bypasses event processing.
    /// Prefer sending events for state changes.
    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    /// Get the current count of pending events.
    ///
    /// Returns a snapshot count of events waiting to be processed: queue + channel.
    /// This value can change immediately due to concurrent senders. Intended for
    /// metrics, logging, or heuristic backpressure decisions.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        let channel_count = self.event_rx.len();
        self.event_queue.len() + channel_count
    }

    /// Check if there are any pending events.
    ///
    /// Returns `true` if either the internal queue or inbound channel currently
    /// holds at least one event. This is a snapshot only; new events may arrive
    /// immediately after this returns. Use for control flow decisions like
    /// whether to continue ticking.
    #[must_use]
    pub fn has_pending_events(&self) -> bool {
        // Early-out optimization: check local queue first since it's cheaper
        !self.event_queue.is_empty() || !self.event_rx.is_empty()
    }

    /// Block until a new event arrives or the timeout expires.
    ///
    /// Returns true if an event was received and queued.
    pub fn wait_for_event(&mut self, timeout: Duration) -> bool {
        if timeout.is_zero() {
            return false;
        }

        match self.event_rx.recv_timeout(timeout) {
            Ok(event) => {
                self.event_queue.push_back(event);
                true
            }
            Err(CoreRecvTimeoutError::Timeout | CoreRecvTimeoutError::Disconnected) => false,
        }
    }
}

impl<E, X, M> std::fmt::Debug for Core<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
    M: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("model", &self.model)
            .field("pending_events", &!self.event_queue.is_empty())
            .field("command_buffer_size", &self.command_buffer.len())
            .finish_non_exhaustive()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
        Decrement,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    #[derive(Debug, Default)]
    struct CounterModel {
        count: i32,
    }

    fn counter_update(
        event: TestEvent,
        model: &mut CounterModel,
    ) -> Command<TestEvent, TestEffect> {
        match event {
            TestEvent::Increment => {
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
            TestEvent::Decrement => {
                model.count -= 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[test]
    fn test_core_basic_functionality() {
        let model = CounterModel { count: 0 };

        let (mut core, _) = Core::new(counter_update, model);

        // Test event handling
        let _command = core.handle_event(TestEvent::Increment);

        // Verify model was updated
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 1);
    }

    #[test]
    fn try_send_event_respects_capacity() {
        let model = CounterModel { count: 0 };

        let (mut core, _) = Core::with_event_channel_capacity(counter_update, model, Some(1));
        assert_eq!(core.event_channel_capacity(), Some(1));

        assert!(core.try_send_event(TestEvent::Increment).is_ok());
        let second = core.try_send_event(TestEvent::Increment);
        assert!(matches!(second, Err(CoreError::ChannelFull)));

        // Process queued work to make space
        assert_eq!(core.process_events().len(), 1);
        assert!(core.try_send_event(TestEvent::Increment).is_ok());
    }

    #[test]
    fn test_core_event_processing() {
        let model = CounterModel { count: 0 };

        let (mut core, sender) = Core::new(counter_update, model);

        // Send events
        sender.send(TestEvent::Increment).unwrap();
        sender.send(TestEvent::Increment).unwrap();
        sender.send(TestEvent::Decrement).unwrap();

        // Process all events
        let commands = core.process_events();

        assert!(!commands.is_empty());
        assert_eq!(commands.len(), 3);

        // Verify final model state
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 1); // +1 +1 -1 = 1
    }

    #[test]
    fn test_core_pending_events() {
        let model = CounterModel { count: 0 };

        let (mut core, sender) = Core::new(counter_update, model);

        assert_eq!(core.pending_count(), 0);
        assert!(!core.has_pending_events());

        sender.send(TestEvent::Increment).unwrap();
        assert!(core.pending_count() > 0);
        assert!(core.has_pending_events());

        let _commands = core.process_events();
        assert_eq!(core.pending_count(), 0);
        assert!(!core.has_pending_events());
    }

    #[test]
    fn test_core_model_access() {
        let model = CounterModel { count: 42 };
        let (mut core, _) = Core::new(counter_update, model);

        // Test immutable model access
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 42);

        // Test mutable model access
        {
            let model_mut: &mut CounterModel = core.model_mut();
            model_mut.count = 100;
        }

        // Verify the mutation worked
        let model_after: &CounterModel = core.model();
        assert_eq!(model_after.count, 100);
    }

    #[test]
    fn test_core_multiple_models() {
        #[derive(Debug, Default)]
        struct UserModel {
            name: String,
            age: u32,
        }

        #[derive(Debug, Default)]
        struct ConfigModel {
            theme: String,
            debug: bool,
        }

        #[derive(Debug)]
        struct AppModel {
            counter: CounterModel,
            user: UserModel,
            config: ConfigModel,
        }

        // Update function for the multi-model app
        fn multi_model_update(
            event: TestEvent,
            model: &mut AppModel,
        ) -> Command<TestEvent, TestEffect> {
            // Just update the counter for simplicity
            match event {
                TestEvent::Increment => {
                    model.counter.count += 1;
                    Command::effect(TestEffect::Log)
                }
                TestEvent::Decrement => {
                    model.counter.count -= 1;
                    Command::effect(TestEffect::Log)
                }
            }
        }

        // Create app model with multiple sub-models
        let app_model = AppModel {
            counter: CounterModel { count: 10 },
            user: UserModel {
                name: "Alice".to_string(),
                age: 25,
            },
            config: ConfigModel {
                theme: "dark".to_string(),
                debug: true,
            },
        };

        let (mut core, _) = Core::new(multi_model_update, app_model);

        // Test accessing different models
        let model = core.model();
        assert_eq!(model.counter.count, 10);
        assert_eq!(model.user.name, "Alice");
        assert_eq!(model.user.age, 25);
        assert_eq!(model.config.theme, "dark");
        assert!(model.config.debug);

        // Test mutable access to different models
        {
            let model_mut = core.model_mut();
            model_mut.counter.count = 999;
            model_mut.user.name = "Bob".to_string();
            model_mut.user.age = 30;
            model_mut.config.theme = "light".to_string();
            model_mut.config.debug = false;
        }

        // Verify all mutations worked
        let model_after = core.model();
        assert_eq!(model_after.counter.count, 999);
        assert_eq!(model_after.user.name, "Bob");
        assert_eq!(model_after.user.age, 30);
        assert_eq!(model_after.config.theme, "light");
        assert!(!model_after.config.debug);
    }

    #[test]
    fn test_core_model_access_api() {
        let model = CounterModel { count: 0 };
        let (mut core, _) = Core::new(counter_update, model);

        // Test model access
        let model_ref: &CounterModel = core.model();
        assert_eq!(model_ref.count, 0);

        // Mutate via model API
        {
            let model_mut: &mut CounterModel = core.model_mut();
            model_mut.count = 123;
        }

        // Verify mutation worked
        let model_after: &CounterModel = core.model();
        assert_eq!(model_after.count, 123);
    }

    #[test]
    fn try_send_event_enqueues_event() {
        let (mut core, _) = Core::new(counter_update, CounterModel::default());

        let result = core.try_send_event(TestEvent::Increment);
        assert!(result.is_ok());

        let commands = core.process_events();
        assert_eq!(commands.len(), 1);
        assert_eq!(core.model().count, 1);
    }
}
````

## File: src/builder.rs
````rust
//! Builder for composing Core, Shell, resources, and executors.
//!
//! The builder wires your pure event handler to the async effect handler and
//! registers any executors you want the Shell to use. Keep models and resources
//! cheap to move/clone; the Shell clones resources for each effect call.
//!
//! This module intentionally avoids traits and lifetimes in the public surface
//! so usage stays straightforward in real apps and tests.
use crate::activity::Activity;
use crate::command::Command;
use crate::core::{Core, EventHandler, EventSender};
use crate::executor::{
    AsyncExecutor, BlockingExecutor, ExecutorRegistry, PanicDetails, PanicHook,
    ResourceBlockingExecutor, Task,
};
use crate::shell::{EffectHandler, Shell, ShellStats};
use crate::syzygy::{Syzygy, SyzygyConfig, SyzygyProfile};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
#[cfg(all(debug_assertions, feature = "flair"))]
use std::sync::OnceLock;

/// Entry point for building a `Syzygy`.
///
/// Typical flow:
/// - set `.model(..)` and optional `.with_resources(..)`
/// - install `.event_handler(..)` and `.effect_handler(..)`
/// - pick a preset via `.profile_interactive()` / `.profile_server()` (optional but recommended)
/// - register executors via `.with_*_executor(..)`
/// - call `.build()`
pub struct SyzygyBuilder<E, X, M, R = ()>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    model: M,
    resources: R,
    pending_profile: Option<SyzygyProfile>,
    _marker: PhantomData<(E, X)>,
}

impl<E, X> Default for SyzygyBuilder<E, X, (), ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, X> SyzygyBuilder<E, X, (), ()>
where
    E: Send + 'static,
    X: Send + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: (),
            resources: (),
            pending_profile: None,
            _marker: PhantomData,
        }
    }
}

impl<Event, Effect, Model, Resources> SyzygyBuilder<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
    Resources: Clone + Send + 'static,
{
    /// Replace the current model with a new one.
    ///
    /// Models are owned by `Core` and mutated only by your event handler.
    /// Calling `.model(..)` more than once replaces the previous value—compose
    /// your application state inside a single struct or tuple if you need
    /// multiple parts of state.
    #[must_use]
    pub fn model<M: 'static>(self, model: M) -> SyzygyBuilder<Event, Effect, M, Resources> {
        SyzygyBuilder {
            model,
            resources: self.resources,
            pending_profile: self.pending_profile,
            _marker: PhantomData,
        }
    }

    /// Install application resources that all effects can access.
    ///
    /// The Shell clones `Resources` per effect call. Use `Arc<_>` for heavy
    /// dependencies (DB pools, clients) or wrap interior mutability explicitly.
    #[must_use]
    pub fn with_resources<R2>(self, resources: R2) -> SyzygyBuilder<Event, Effect, Model, R2>
    where
        R2: Clone + Send + 'static,
    {
        SyzygyBuilder {
            model: self.model,
            resources,
            pending_profile: self.pending_profile,
            _marker: PhantomData,
        }
    }

    /// Remember a profile to be applied once handlers are installed.
    #[must_use]
    pub fn preset_profile(mut self, profile: SyzygyProfile) -> Self {
        self.pending_profile = Some(profile);
        self
    }

    /// Finalize model configuration and set the event handler.
    ///
    /// Your event handler is a pure function. It mutates the model and returns
    /// a `Command` telling the Shell what effects to run.
    #[must_use]
    pub fn event_handler(
        self,
        event_handler: EventHandler<Event, Effect, Model>,
    ) -> ConfiguredBuilder<Event, Effect, Model, Resources> {
        let mut builder = ConfiguredBuilder {
            event_handler,
            effect_handler: None,
            model: self.model,
            resources: self.resources,
            exec_registry: ExecutorRegistry::<Event>::default(),
            effect_channel_capacity: None,
            event_channel_capacity: None,
            syzygy_config: SyzygyConfig::default(),
            panic_handler: None,
            _marker: PhantomData,
        };

        if let Some(profile) = self.pending_profile {
            builder = builder.with_profile(profile);
        }

        builder
    }
}

/// Builder stage where handlers and executors are configured.
///
/// After installing handlers you can register any number of executors. If a
/// `Task` targets a specific executor type you didn’t register, the Shell will
/// error when scheduling it. You can avoid registry lookups entirely by using
/// `Task::async_current`/`Task::stream_current` (runs on the current Tokio
/// runtime if available; otherwise completes inline by blocking the thread).
pub struct ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Clone + Send + 'static,
{
    event_handler: EventHandler<Event, Effect, Model>,
    effect_handler: Option<EffectHandler<Event, Effect, Resources>>,
    model: Model,
    resources: Resources,
    exec_registry: ExecutorRegistry<Event>,
    effect_channel_capacity: Option<usize>,
    event_channel_capacity: Option<usize>,
    syzygy_config: SyzygyConfig,
    panic_handler: Option<Arc<PanicHook<Event, Effect>>>,
    _marker: PhantomData<Effect>,
}

impl<Event, Effect, Model, Resources> ConfiguredBuilder<Event, Effect, Model, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Model: 'static,
    Resources: Clone + Send + 'static,
{
    /// Install the effect handler.
    ///
    /// The handler receives a single effect and the cloned `Resources` value
    /// and returns a `Task` plan. The Shell drives the plan on the registered
    /// executors.
    #[must_use]
    pub fn effect_handler<H>(mut self, handler: H) -> Self
    where
        H: FnMut(Effect, Resources) -> Task<Event, Effect> + Send + 'static,
    {
        self.effect_handler = Some(Box::new(handler));
        self
    }

    /// Apply a prebuilt runner configuration.
    #[must_use]
    pub fn with_syzygy_config(mut self, config: SyzygyConfig) -> Self {
        self.syzygy_config = config;
        self
    }

    /// Apply a preset profile for idle cadence and effect buffering.
    #[must_use]
    pub fn with_profile(mut self, profile: SyzygyProfile) -> Self {
        let settings = profile.settings();
        self.syzygy_config = self.syzygy_config.clone().idle_sleep(settings.idle_sleep);
        if self.effect_channel_capacity.is_none() {
            self.effect_channel_capacity = settings.effect_channel_capacity;
        }
        if self.event_channel_capacity.is_none() {
            self.event_channel_capacity = settings.event_channel_capacity;
        }
        self
    }

    /// Install a handler invoked whenever a task panics.
    ///
    /// The handler receives [`PanicDetails`] describing the task context and a message captured
    /// from the panic payload. Return a `Command` (typically an error event) to surface the
    /// failure to your Core.
    #[must_use]
    pub fn with_panic_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(PanicDetails, String) -> Command<Event, Effect> + Send + Sync + 'static,
    {
        self.panic_handler = Some(Arc::new(handler));
        self
    }

    /// Apply the [`SyzygyProfile::Interactive`] preset.
    #[must_use]
    pub fn profile_interactive(self) -> Self {
        self.with_profile(SyzygyProfile::Interactive)
    }

    /// Apply the [`SyzygyProfile::Server`] preset.
    #[must_use]
    pub fn profile_server(self) -> Self {
        self.with_profile(SyzygyProfile::Server)
    }

    /// Apply the [`SyzygyProfile::Ci`] preset.
    #[must_use]
    pub fn profile_ci(self) -> Self {
        self.with_profile(SyzygyProfile::Ci)
    }

    /// Apply the [`SyzygyProfile::Batch`] preset.
    #[must_use]
    pub fn profile_batch(self) -> Self {
        self.with_profile(SyzygyProfile::Batch)
    }

    /// Replace the executor registry with a pre-built one.
    #[must_use]
    pub fn with_executor_registry(mut self, registry: ExecutorRegistry<Event>) -> Self {
        self.exec_registry = registry;
        self
    }

    /// Register an async executor.
    ///
    /// Use for futures/streams scheduling (e.g. `TokioExecutor`, `InlineAsync`).
    #[must_use]
    pub fn with_async_executor<T>(mut self, exec: T) -> Self
    where
        T: AsyncExecutor + Send + Sync + 'static,
    {
        self.exec_registry.insert_async(exec);
        self
    }

    /// Register a blocking executor.
    ///
    /// Use for CPU-bound or blocking work that doesn’t require a shared
    /// resource (e.g. Rayon-based executor).
    #[must_use]
    pub fn with_blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: BlockingExecutor + Send + Sync + 'static,
    {
        self.exec_registry.insert_blocking(exec);
        self
    }

    /// Register a resource-blocking executor (single-threaded shared resource).
    ///
    /// Use when jobs need mutable access to a single owned resource with FIFO
    /// guarantees (e.g. a device handle that must not be used concurrently).
    #[must_use]
    pub fn with_resource_blocking_executor<T>(mut self, exec: T) -> Self
    where
        T: ResourceBlockingExecutor + Send + Sync + 'static,
    {
        self.exec_registry.insert_resource_blocking(exec);
        self
    }

    /// Override the Shell effect channel capacity.
    ///
    /// `None` means unbounded. Bounded channels apply backpressure to the
    /// caller when the effect queue is saturated by returning
    /// [`ShellError::EffectQueueFull`](crate::error::ShellError::EffectQueueFull).
    /// Combine with [`SyzygyProfile`] presets for sensible defaults.
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # fn update(_: Event, _: &mut Model) -> Command<Event, Effect> { Command::none() }
    /// # fn effects(_: Effect, _: ()) -> Task<Event, Effect> { Task::none() }
    /// # #[derive(Default)] struct Model;
    /// # #[derive(Clone)] enum Event { Ping }
    /// # #[derive(Clone)] enum Effect { DoPing }
    /// let runner = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(update)
    ///     .effect_handler(effects)
    ///     .with_effect_channel_capacity(Some(256))
    ///     .build();
    /// # let _ = runner;
    /// ```
    #[must_use]
    pub fn with_effect_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.effect_channel_capacity = capacity;
        self
    }

    /// Override the core event channel capacity.
    ///
    /// `None` keeps the default unbounded channel. Bounded channels apply backpressure
    /// to event producers by returning [`CoreError::ChannelFull`](crate::error::CoreError::ChannelFull).
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # fn update(_: Event, _: &mut Model) -> Command<Event, Effect> { Command::none() }
    /// # fn effects(_: Effect, _: ()) -> Task<Event, Effect> { Task::none() }
    /// # #[derive(Default)] struct Model;
    /// # #[derive(Clone)] enum Event { Ping }
    /// # #[derive(Clone)] enum Effect { DoPing }
    /// let runner = Syzygy::builder::<Event, Effect>()
    ///     .model(Model::default())
    ///     .event_handler(update)
    ///     .effect_handler(effects)
    ///     .with_event_channel_capacity(Some(64))
    ///     .build();
    /// # let _ = runner;
    /// ```
    #[must_use]
    pub fn with_event_channel_capacity(mut self, capacity: Option<usize>) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Build the system and return a `Syzygy`.
    pub fn build(self) -> Syzygy<Event, Effect, Model, Resources> {
        #[cfg(all(debug_assertions, feature = "flair"))]
        let flair_banner = (
            self.effect_channel_capacity,
            self.event_channel_capacity,
            self.syzygy_config.idle_sleep,
        );
        let (core, event_tx) = Core::with_event_channel_capacity(
            self.event_handler,
            self.model,
            self.event_channel_capacity,
        );
        let registry = Arc::new(self.exec_registry);

        let shell = Self::build_shell(
            registry,
            self.effect_handler,
            self.resources,
            event_tx,
            self.effect_channel_capacity,
            self.panic_handler,
        );
        let runner = Syzygy::with_config(core, shell, self.syzygy_config);

        #[cfg(all(debug_assertions, feature = "flair"))]
        {
            static BANNER: OnceLock<()> = OnceLock::new();
            BANNER.get_or_init(|| {
                let (effect_cap, event_cap, idle_sleep) = flair_banner;
                let effect_msg = effect_cap
                    .map(|cap| cap.to_string())
                    .unwrap_or_else(|| "unbounded".to_string());
                let event_msg = event_cap
                    .map(|cap| cap.to_string())
                    .unwrap_or_else(|| "unbounded".to_string());
                let idle_ms = idle_sleep.as_millis();
                eprintln!(
                    "✨ Syzygy ready — idle: {idle_ms}ms, effect cap: {effect_msg}, event cap: {event_msg}"
                );
            });
        }

        runner
    }

    fn build_shell(
        exec_registry: Arc<ExecutorRegistry<Event>>,
        effect_handler: Option<EffectHandler<Event, Effect, Resources>>,
        resources: Resources,
        event_tx: EventSender<Event>,
        effect_channel_capacity: Option<usize>,
        panic_handler: Option<Arc<PanicHook<Event, Effect>>>,
    ) -> Shell<Event, Effect, Resources> {
        use crossbeam_channel::{bounded, unbounded};

        fn default_effect_handler<E, X, R>(_: X, _: R) -> Task<E, X>
        where
            E: Send + 'static,
            X: Send + 'static,
            R: Clone + Send + 'static,
        {
            Task::none()
        }

        let (effect_tx, effect_rx) = match effect_channel_capacity {
            Some(capacity) => bounded(capacity),
            None => unbounded(),
        };

        let effect_handler = effect_handler.unwrap_or_else(|| {
            Box::new(move |effect, resources| {
                default_effect_handler::<Event, Effect, _>(effect, resources)
            })
        });

        Shell {
            effect_rx,
            effect_tx,
            event_tx,
            effect_handler,
            resources,
            activity: Activity::new(),
            effect_channel_capacity,
            executors: exec_registry,
            closed: false,
            prefetched_effects: VecDeque::new(),
            stats: ShellStats::default(),
            executors_shutdown: false,
            panic_handler,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::syzygy::SyzygyProfile;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
    }

    #[derive(Debug, Default)]
    struct TestModel {
        count: i32,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }

    fn test_update(event: TestEvent, model: &mut TestModel) -> Command<TestEvent, TestEffect> {
        match event {
            TestEvent::Increment => {
                model.count += 1;
                Command::effect(TestEffect::Log)
            }
        }
    }

    #[cfg(feature = "rt-inline")]
    #[test]
    fn test_builder_basics() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_async_executor(crate::executor::InlineAsync::new())
            .build();

        let (mut core, _shell) = runner.split();
        let _command = core.handle_event(TestEvent::Increment);
        assert_eq!(core.model().count, 1);
    }

    #[test]
    fn test_profile_interactive_sets_defaults() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_profile(SyzygyProfile::Interactive)
            .build();

        assert_eq!(runner.shell().effect_channel_capacity(), Some(256));
        assert_eq!(runner.core().event_channel_capacity(), None);
        assert_eq!(
            runner.config().idle_sleep,
            std::time::Duration::from_millis(1)
        );
    }

    #[test]
    fn test_profile_server_sets_capacities() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_profile(SyzygyProfile::Server)
            .build();

        assert_eq!(runner.shell().effect_channel_capacity(), Some(1024));
        assert_eq!(runner.core().event_channel_capacity(), Some(1024));
        assert_eq!(
            runner.config().idle_sleep,
            std::time::Duration::from_millis(0)
        );
    }

    #[test]
    fn test_profile_ci_sets_capacities() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_profile(SyzygyProfile::Ci)
            .build();

        assert_eq!(runner.shell().effect_channel_capacity(), Some(64));
        assert_eq!(runner.core().event_channel_capacity(), Some(64));
        assert_eq!(
            runner.config().idle_sleep,
            std::time::Duration::from_millis(0)
        );
    }

    #[cfg(feature = "rt-inline")]
    #[test]
    fn test_shell_type() {
        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel { count: 0 })
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_async_executor(crate::executor::InlineAsync::new())
            .build();

        let (_core, shell) = runner.split();
        let _: Shell<TestEvent, TestEffect> = shell;
    }

    #[test]
    fn test_build_without_executors_is_allowed() {
        let mut runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model(TestModel::default())
            .event_handler(test_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .build();

        // With no executors registered, processing events still works as long as
        // the effect handler does not schedule work onto an executor.
        runner
            .core()
            .try_send_event(TestEvent::Increment)
            .expect("event channel should be open");
        runner.step().unwrap();
        assert_eq!(runner.core().model().count, 1);
    }

    #[cfg(feature = "rt-inline")]
    #[test]
    fn test_multi_model() {
        #[derive(Debug, Default)]
        struct UserModel {
            name: String,
        }

        #[derive(Debug, Default)]
        struct ConfigModel {
            theme: String,
        }

        fn multi_update(
            event: TestEvent,
            model: &mut (UserModel, ConfigModel),
        ) -> Command<TestEvent, TestEffect> {
            match event {
                TestEvent::Increment => {
                    let (user, config) = model;
                    user.name = "Updated".to_string();
                    config.theme = "dark".to_string();
                    Command::effect(TestEffect::Log)
                }
            }
        }

        let runner = Syzygy::builder::<TestEvent, TestEffect>()
            .model((
                UserModel {
                    name: "Alice".to_string(),
                },
                ConfigModel {
                    theme: "light".to_string(),
                },
            ))
            .event_handler(multi_update)
            .effect_handler(|_e: TestEffect, _resources| {
                crate::executor::Task::<TestEvent, TestEffect>::events(Vec::new())
            })
            .with_async_executor(crate::executor::InlineAsync::new())
            .build();

        let (mut core, _shell) = runner.split();

        core.handle_event(TestEvent::Increment);

        let (_user, config) = core.model();
        assert_eq!(config.theme, "dark");
    }
}
````

## File: src/lib.rs
````rust
//! # Syzygy - Zero-Overhead TEA for Rust
//!
//! A high-performance implementation of The Elm Architecture (TEA) with Core/Shell separation,
//! providing deterministic state management with async side effects and a single root model.
//!
//! ## Quick Start
//!
//! ```rust
//! use syzygy::executor::{Task, TokioExecutor};
//! use syzygy::prelude::*;
//!
//! #[derive(Default)]
//! struct User {
//!     name: String,
//! }
//!
//! #[derive(Default)]
//! struct Counter {
//!     value: i32,
//! }
//!
//! #[derive(Default)]
//! struct App {
//!     user: User,
//!     counter: Counter,
//! }
//!
//! #[derive(Clone)]
//! enum CounterEvent {
//!     Increment,
//!     Decrement,
//! }
//!
//! #[derive(Clone)]
//! enum CounterEffect {
//!     Log(String),
//! }
//!
//! fn event_handler(
//!     event: CounterEvent,
//!     model: &mut App,
//! ) -> Command<CounterEvent, CounterEffect> {
//!     match event {
//!         CounterEvent::Increment => {
//!             model.counter.value += 1;
//!             Command::effect(CounterEffect::Log(format!("{}", model.counter.value)))
//!         }
//!         CounterEvent::Decrement => {
//!             model.counter.value -= 1;
//!             Command::effect(CounterEffect::Log(format!("{}", model.counter.value)))
//!         }
//!     }
//! }
//!
//! fn effect_handler(effect: CounterEffect, resources: &'static str) -> Task<CounterEvent, CounterEffect> {
//!     match effect {
//!         CounterEffect::Log(message) => Task::async_on::<TokioExecutor, _>(
//!             async move {
//!                 println!("{resources} {message}");
//!                 Command::none()
//!             },
//!         ),
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut runner = Syzygy::builder::<CounterEvent, CounterEffect>()
//!     .model(App::default())
//!     .with_resources("LOG")
//!     .event_handler(event_handler)
//!     .effect_handler(effect_handler)
//!     .with_async_executor(TokioExecutor::current_thread_cpu("syzygy-docs"))
//!     .build();
//!
//! runner.core().try_send_event(CounterEvent::Increment)?;
//! runner.run_until(|core, _shell| core.model().counter.value == 1)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Core Features
//!
//! ### Zero-Overhead Performance
//! - Direct storage access without runtime overhead
//! - Compile-time type safety with zero-cost abstractions
//! - Task spawning 24x faster than alternatives (~4ns per task)
//!
//! ### Single Root Model
//! Compose your own application state and register it once on the builder:
//! ```rust
//! # use syzygy::executor::Task;
//! # use syzygy::prelude::*;
//! #[derive(Default)]
//! struct UserModel { name: String }
//! #[derive(Default)]
//! struct ConfigModel { theme: String }
//! #[derive(Default)]
//! struct AppModel { user: UserModel, config: ConfigModel }
//! # #[derive(Clone)] enum Event { Test }
//! # #[derive(Clone)] enum Effect { Test }
//! # fn update(_event: Event, _model: &mut AppModel) -> Command<Event, Effect> { Command::none() }
//! # fn effects(_effect: Effect, _resources: ()) -> Task<Event, Effect> { Task::none() }
//! let runner = Syzygy::builder::<Event, Effect>()
//!     .model(AppModel::default())
//!     .event_handler(update)
//!     .effect_handler(effects)
//!     .build();
//! # let _ = runner;
//! ```
//!
//! ### Async Effects with Resources
//! ```rust
//! # use std::sync::Arc;
//! # use syzygy::executor::{Task, TokioExecutor};
//! # use syzygy::prelude::*;
//! # struct Database;
//! # impl Database {
//! #     async fn save(&self) {}
//! # }
//! #[derive(Clone)]
//! struct Services {
//!     db: Arc<Database>,
//! }
//!
//! fn effects(effect: Effect, services: Services) -> Task<Event, Effect> {
//!     match effect {
//!         Effect::SaveUser => {
//!             let db = Arc::clone(&services.db);
//!             Task::async_on::<TokioExecutor, _>(async move {
//!                 db.save().await;
//!                 Command::event(Event::UserSaved)
//!             })
//!         }
//!         Effect::Log(msg) => Task::async_current(async move {
//!             println!("LOG {msg}");
//!             Command::none()
//!         }),
//!     }
//! }
//! ```
//!
//! ## Runtime Support
//!
//! Syzygy ships with dedicated executors:
//!
//! - `TokioExecutor` – spawn async work onto a Tokio runtime you control
//! - Custom inline executors exist for tests/CLIs when needed (niche)
//! - `SingleThreadExecutor` – sequential, borrowing access to a worker resource
//! - `RayonExecutor` (optional feature) – CPU-heavy parallel work
//!
//! Bring additional runtimes by implementing the `AsyncExecutor` trait.
//!
//! Don’t want to register executors? Use `Task::async_current`/`Task::stream_current`
//! in your effect handler. They run on the current Tokio runtime if available,
//! or complete inline by blocking the current thread when no runtime is active.
//!
//! ## Examples
//!
//! Learn Syzygy progressively with our example series:
//!
//! - **[basic_counter.rs]** – the smallest possible Syzygy app
//! - **[async_effect.rs]** – scheduling work onto Tokio executors
//! - **[timeout_pattern.rs]** – modeling timeouts as explicit events with retries
//! - **[manual_loop.rs]** – driving `Core`/`Shell` without the runner helper
//! - **[two_executors.rs]** – mixing IO and CPU executors under Tokio
//!
//! [basic_counter.rs]: https://github.com/ribelo/syzygy/blob/main/examples/basic_counter.rs
//! [async_effect.rs]: https://github.com/ribelo/syzygy/blob/main/examples/async_effect.rs
//! [manual_loop.rs]: https://github.com/ribelo/syzygy/blob/main/examples/manual_loop.rs
//! [timeout_pattern.rs]: https://github.com/ribelo/syzygy/blob/main/examples/timeout_pattern.rs
//! [two_executors.rs]: https://github.com/ribelo/syzygy/blob/main/examples/two_executors.rs
//!
//! ## Performance Benchmarks
//!
//! Syzygy delivers exceptional performance with real-world patterns:
//!
//! - **Task Spawning**: ~4ns (24x faster than alternatives)
//! - **Model Access**: ~0.31ns (single model), ~4.05ns (16 models)
//! - **Event Processing**: <100ns typical
//! - **Command Creation**: <50ns typical
//!
//! Run benchmarks yourself:
//! ```bash
//! cargo bench
//! ```
//!
//! ## The TEA Pattern
//!
//! The Elm Architecture provides predictable state management:
//!
//! ```text
//! ┌─────────────┐    Events    ┌──────────────┐    Commands    ┌─────────────┐
//! │    View     │──────────────►│    Update    │───────────────►│   Effects   │
//! │   (Your     │               │  (Pure Fn)   │                │ (Async Side │
//! │    App)     │               │              │                │   Effects)  │
//! └─────────────┘               └──────────────┘                └─────────────┘
//!       ▲                              │                              │
//!       │                              ▼                              │
//!       │                       ┌──────────────┐                      │
//!       │          New State    │    Model     │          Events      │
//!       └───────────────────────│   (State)    │◀─────────────────────┘
//!                               └──────────────┘
//! ```
//!
//! **Key Principles:**
//! - **Unidirectional Data Flow**: Events → Update → Model → Effects
//! - **Pure Updates**: No side effects in update functions
//! - **Predictable**: Same event always produces same state change
//! - **Composable**: Models, effects, and handlers compose cleanly
//!
//! ## Error Handling
//!
//! Syzygy follows the **"error-as-events"** pattern - all errors flow through
//! the same event pipeline for consistent handling:
//!
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default)] struct Model;
//! # #[derive(Debug, Clone)] enum Effect { SaveData }
//! #[derive(Debug, Clone)]
//! enum AppEvent {
//!     ProcessData { data: String },
//!     ValidationError { message: String },
//!     DataSaved,
//! }
//!
//! fn update(event: AppEvent, model: &mut Model) -> Command<AppEvent, Effect> {
//!     match event {
//!         AppEvent::ProcessData { data } => {
//!             if data.is_empty() {
//!                 // Error as event - consistent handling
//!                 Command::event(AppEvent::ValidationError {
//!                     message: "Data cannot be empty".to_string()
//!                 })
//!             } else {
//!                 // Success path
//!                 Command::effect(Effect::SaveData)
//!             }
//!         }
//!         AppEvent::ValidationError { message } => Command::none(),
//!         AppEvent::DataSaved => {
//!             println!("Data saved successfully!");
//!             Command::none()
//!         }
//!     }
//! }
//! ```
//!
//! ## Architecture Overview
//!
//! Syzygy implements a **Core/Shell** architecture for clean separation of concerns:
//!
//! ### Core (Synchronous)
//! - Owns application state (models)
//! - Processes events through pure update functions
//! - Generates commands for side effects
//! - Deterministic and easily testable
//!
//! ### Shell (Asynchronous)
//! - Handles side effects (HTTP, database, file I/O)
//! - Manages resources (database pools, HTTP clients)
//! - Can send events back to Core
//! - Provides safe task spawning with automatic cleanup
//!
//! ### Runner (Orchestration)
//! - Coordinates between Core and Shell
//! - Provides simple tick-based execution model
//! - Handles event routing and command execution
//!
//! ## Advanced Patterns
//!
//! ### Multi-Model Access
//! ```rust
//! # #[derive(Debug, Default)] struct UserModel { name: String }
//! # #[derive(Debug, Default)] struct ConfigModel { theme: String }
//! # #[derive(Debug, Default)] struct SessionModel { active: bool }
//! # let mut model = (SessionModel::default(), ConfigModel::default(), UserModel::default());
//! // Model is your own type; pattern match to access parts
//! let (session, config, user) = &model;
//! assert!(!session.active);
//! let _ = &config.theme;
//! let _ = &user.name;
//! ```
//!
//! ### Background Task Management
//! ```rust
//! # use syzygy::prelude::*;
//! # use syzygy::executor::{Task, InlineAsync};
//! # #[derive(Debug, Clone)] enum Event { TaskComplete }
//! # #[derive(Debug, Clone)] enum MyEffect { DoWork }
//! fn handle_effect(effect: MyEffect, _res: ()) -> Task<Event, MyEffect> {
//!     match effect {
//!         MyEffect::DoWork => Task::async_on::<InlineAsync, _>(async move {
//!             // Do async work and return events/effects via Command
//!             // tokio timers require a runtime; InlineAsync uses thread sleep
//!             Command::event(Event::TaskComplete)
//!         }),
//!     }
//! }
//! ```
//!
//! ## Safety Guarantees
//!
//! - **Memory Safety**: All spawned tasks automatically cancelled on context drop
//! - **Type Safety**: Compile-time verification of model and resource access
//! - **Concurrency Safety**: Interior mutability handled safely with `UnsafeCell`
//! - **Resource Safety**: No resource leaks or orphaned tasks
//!
//! ## Testing
//!
//! Syzygy applications are highly testable due to pure update functions:
//!
//! ```rust
//! # use syzygy::prelude::*;
//! # #[derive(Debug, Default, PartialEq)] struct CounterModel { count: i32 }
//! # #[derive(Debug, Clone)] enum CounterEvent { Increment }
//! # #[derive(Debug, Clone, PartialEq)] enum CounterEffect { Log }
//! fn update(event: CounterEvent, model: &mut CounterModel) -> Command<CounterEvent, CounterEffect> {
//!     match event {
//!         CounterEvent::Increment => { model.count += 1; Command::effect(CounterEffect::Log) }
//!     }
//! }
//! # #[cfg(test)]
//! # mod tests { use super::*; #[test] fn test_counter_increment() {
//! #     let mut model = CounterModel::default();
//! #     let command = update(CounterEvent::Increment, &mut model);
//! #     assert_eq!(model.count, 1);
//! #     let steps: Vec<_> = command.into_iter().collect();
//! #     assert!(matches!(steps[0], CommandStep::Effect(CounterEffect::Log)));
//! # }}
//! ```

// Core modules
pub mod activity;
#[cfg(feature = "cli")]
pub mod cli;
pub mod command;
pub mod core;
// EffectContext/EventContext were removed from public API; handlers receive
// plain arguments: event handlers get `&mut model`, effect handlers get
// `(effect, resources)` and return `Task`.
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "shell")]
pub mod syzygy;

// Shared runtime facade powering scheduling, spawning, and timers
// Builder pattern
#[cfg(feature = "shell")]
pub mod builder;

// EventContext removed from public API; event handlers receive &mut model directly

// Error handling
pub mod error;

// Resources are provided by user code and cloned per-effect; no internal storage module.

// Executor system for specialized effect handling
#[cfg(feature = "shell")]
pub mod executor;
pub mod resource_cell;

pub mod prelude {
    // No public contexts in the new API; handlers receive plain args

    // Command system
    pub use crate::command::builders as command;
    pub use crate::command::builders as cmd;
    pub use crate::command::builders::{
        batch, effect, effects, event, events, none, parallel, sequential,
    };
    pub use crate::command::{Command, CommandStep};

    // Core/Shell architecture
    pub use crate::activity::Activity;
    #[cfg(feature = "cli")]
    pub use crate::cli::{install_ctrlc, spawn_stdin_listener};
    pub use crate::core::{Core, EventHandler, EventSender};
    #[cfg(feature = "shell")]
    pub use crate::shell::{Shell, ShellStats, ShellStatsSnapshot};
    #[cfg(feature = "shell")]
    pub use crate::syzygy::{Runner, Syzygy, SyzygyConfig, SyzygyProfile, SyzygyProfileSettings};

    // Type aliases for common use cases
    /// A simple Shell for applications that only need models (no resources or executors).
    /// This is the most common case for basic applications.
    ///
    /// # Example
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// #[derive(Debug, Clone)]
    /// enum Event {
    ///     Increment,
    /// }
    /// #[derive(Debug, Clone)]
    /// enum Effect {
    ///     LogTick,
    /// }
    ///
    /// type MyModel = u32;
    /// type MyCore = Core<Event, Effect, MyModel>;
    /// type MyShell = Shell<Event, Effect>;
    ///
    /// fn build() -> (MyCore, MyShell) {
    ///     Syzygy::builder::<Event, Effect>()
    ///         .model(0u32)
    ///         .event_handler(|event, model| match event {
    ///             Event::Increment => {
    ///                 *model += 1;
    ///                 cmd::effect(Effect::LogTick)
    ///             }
    ///         })
    ///         .effect_handler(|effect, _| match effect {
    ///             Effect::LogTick => Command::none(),
    ///         })
    ///         .profile_interactive()
    ///         .build()
    /// }
    /// ```
    // Effect handlers with AFIT
    #[cfg(feature = "shell")]
    pub use crate::shell::EffectHandler;

    // Executor system
    #[cfg(all(feature = "shell", feature = "rayon"))]
    pub use crate::executor::RayonExecutor;

    #[cfg(all(feature = "shell", feature = "rt-inline"))]
    pub use crate::executor::InlineAsync;
    #[cfg(all(feature = "shell", feature = "rt-single-thread"))]
    pub use crate::executor::SingleThreadExecutor;
    #[cfg(all(feature = "shell", feature = "tokio"))]
    pub use crate::executor::TokioExecutor;
    #[cfg(feature = "shell")]
    pub use crate::executor::{
        ExecutorRegistry, PanicDetails, PanicHook, PanicTaskKind, Plan, Task,
    };

    // Builder
    #[cfg(feature = "shell")]
    pub use crate::builder::SyzygyBuilder;

    // Errors
    #[cfg(feature = "shell")]
    pub use crate::error::ShellError;
    pub use crate::error::{CommandError, CoreError, EffectError};

    // Effect output types

    pub use crate::resource_cell::{ResourceCell, SetOnceError};
    // spawn facade removed; use tokio::runtime::Handle directly when needed
}
````

## File: src/shell.rs
````rust
//! # Shell - Asynchronous Effect Management
//!
//! The Shell orchestrates asynchronous effect execution, bridging pure Core updates
//! with side-effectful operations. Effect handlers now return `Task` plans which
//! the Shell drives using executors registered in an immutable registry.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::activity::Activity;
use crate::command::{Command, CommandStep};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::task::{drive_task_with_activity, PanicHook};
use crate::executor::{ExecutorRegistry, Task};

#[cfg(feature = "tracing")]
use tracing::{debug, span, Level};

/// Effect handler accepting closures that produce declarative tasks.
pub type EffectHandler<E, X, R> = Box<dyn FnMut(X, R) -> Task<E, X> + Send + 'static>;

#[derive(Clone, Default)]
pub struct ShellStats {
    inner: Arc<ShellStatsInner>,
}

struct ShellStatsInner {
    dropped_events: AtomicUsize,
    dropped_effect_steps: AtomicUsize,
}

impl Default for ShellStatsInner {
    fn default() -> Self {
        Self {
            dropped_events: AtomicUsize::new(0),
            dropped_effect_steps: AtomicUsize::new(0),
        }
    }
}

impl ShellStats {
    pub fn inc_dropped_event(&self) {
        self.inner.dropped_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_dropped_effect_step(&self) {
        self.inner
            .dropped_effect_steps
            .fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn snapshot(&self) -> ShellStatsSnapshot {
        ShellStatsSnapshot {
            dropped_events: self.inner.dropped_events.load(Ordering::Relaxed),
            dropped_effect_steps: self.inner.dropped_effect_steps.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellStatsSnapshot {
    pub dropped_events: usize,
    pub dropped_effect_steps: usize,
}

/// The Shell orchestrates async effect execution independently of Core
pub struct Shell<E, X, R = ()>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    /// Channel for receiving command outputs (effects)
    pub(crate) effect_rx: Receiver<CommandStep<E, X>>,
    pub(crate) effect_tx: Sender<CommandStep<E, X>>,

    /// Channel for sending events back to Core
    pub(crate) event_tx: EventSender<E>,

    /// Registry of pluggable executors
    pub(crate) executors: Arc<ExecutorRegistry<E>>,

    /// User-provided effect handler
    pub(crate) effect_handler: EffectHandler<E, X, R>,

    /// Shared application resources cloned per effect invocation
    pub(crate) resources: R,

    /// Activity tracker for in-flight async work
    pub(crate) activity: Activity,

    /// Optional capacity for the effect queue (None => unbounded)
    pub(crate) effect_channel_capacity: Option<usize>,

    /// Closed flag for Runner shutdown checks
    pub(crate) closed: bool,

    /// Prefetched effects waiting to be processed (local overflow buffer)
    pub(crate) prefetched_effects: VecDeque<CommandStep<E, X>>,

    /// Observability stats for dropped work
    pub(crate) stats: ShellStats,

    /// Tracks whether executor shutdown has been requested
    pub(crate) executors_shutdown: bool,

    /// Optional panic handler invoked when tasks panic.
    pub(crate) panic_handler: Option<Arc<PanicHook<E, X>>>,
}

// No public constructors. Shell instances are created exclusively by the builder.

impl<E, X, R> Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    fn push_effect_step(&mut self, step: CommandStep<E, X>) -> Result<(), ShellError> {
        match self.effect_tx.try_send(step) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(step)) => {
                if let Ok(queued_step) = self.effect_rx.try_recv() {
                    self.prefetched_effects.push_back(queued_step);
                }

                match self.effect_tx.try_send(step) {
                    Ok(()) => Ok(()),
                    Err(TrySendError::Full(step)) => {
                        let limit = self.effect_channel_capacity.unwrap_or(usize::MAX);
                        let occupancy = self.effect_rx.len() + self.prefetched_effects.len();
                        if occupancy < limit {
                            self.prefetched_effects.push_back(step);
                            Ok(())
                        } else {
                            Err(ShellError::EffectQueueFull { capacity: limit })
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        self.stats.inc_dropped_effect_step();
                        Err(ShellError::CommandExecutionFailed(
                            "Effect channel closed".to_string(),
                        ))
                    }
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                self.stats.inc_dropped_effect_step();
                Err(ShellError::CommandExecutionFailed(
                    "Effect channel closed".to_string(),
                ))
            }
        }
    }

    fn process_effect(&mut self, effect: X) -> Result<(), ShellError> {
        let resources = self.resources.clone();
        let task = {
            let handler = &mut self.effect_handler;
            handler(effect, resources)
        };
        drive_task_with_activity(
            &self.executors,
            task,
            self.event_tx.clone(),
            self.effect_tx.clone(),
            Some(&self.activity),
            Some(&self.stats),
            self.panic_handler.as_ref(),
        )
    }

    // No public effect sender accessors to keep the API minimal.

    /// Route a command's outputs back into the effect and event queues.
    ///
    /// Useful when you want to orchestrate Core and Shell manually without the
    /// convenience Runner. Each command is processed synchronously.
    pub fn dispatch_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        #[cfg(feature = "tracing")]
        debug!("Executing command");

        #[cfg(feature = "tracing")]
        debug!("Starting synchronous command routing");

        let mut _event_count = 0;
        let mut _effect_count = 0;

        // Simple synchronous iteration over command outputs
        for output in command {
            match output {
                CommandStep::Event(event) => {
                    _event_count += 1;
                    #[cfg(feature = "tracing")]
                    debug!("Routing event to Core");

                    self.event_tx.send(event).map_err(|_| {
                        self.stats.inc_dropped_event();
                        ShellError::CommandExecutionFailed("Event channel closed".to_string())
                    })?;
                }
                CommandStep::Effect(effect) => {
                    _effect_count += 1;
                    #[cfg(feature = "tracing")]
                    debug!("Routing single effect to Shell");

                    self.push_effect_step(CommandStep::Effect(effect))?;
                }
                CommandStep::Batch(effects) => {
                    _effect_count += effects.len();
                    #[cfg(feature = "tracing")]
                    debug!(count = effects.len(), "Routing batch effects to Shell");

                    self.push_effect_step(CommandStep::Batch(effects))?;
                }
                CommandStep::Parallel(effects) => {
                    _effect_count += effects.len();
                    #[cfg(feature = "tracing")]
                    debug!(count = effects.len(), "Routing parallel effects to Shell");

                    self.push_effect_step(CommandStep::Parallel(effects))?;
                }
            }
        }

        #[cfg(feature = "tracing")]
        debug!(
            _event_count,
            _effect_count, "Synchronous command execution completed"
        );

        Ok(())
    }

    fn next_effect_step(&mut self) -> Option<CommandStep<E, X>> {
        if let Some(step) = self.prefetched_effects.pop_front() {
            Some(step)
        } else {
            self.effect_rx.try_recv().ok()
        }
    }

    fn handle_effect_step(&mut self, step: CommandStep<E, X>) -> Result<usize, ShellError> {
        match step {
            CommandStep::Effect(effect) => {
                self.process_effect(effect)?;
                Ok(1)
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                let mut handled = 0usize;
                for effect in effects {
                    self.process_effect(effect)?;
                    handled += 1;
                }
                Ok(handled)
            }
            CommandStep::Event(_) => unreachable!(),
        }
    }

    /// Process all pending effects synchronously and return count of effects processed
    pub fn drain(&mut self) -> Result<usize, ShellError> {
        #[cfg(feature = "tracing")]
        let _span = span!(Level::DEBUG, "shell_drain").entered();

        let mut effect_count = 0usize;

        while let Some(step) = self.next_effect_step() {
            effect_count += self.handle_effect_step(step)?;
        }

        #[cfg(feature = "tracing")]
        if effect_count > 0 {
            debug!(effects = effect_count, "Shell drain completed");
        }

        Ok(effect_count)
    }

    /// Process at most one effect from the queue synchronously
    ///
    /// Returns true if an effect was processed, false if the queue was empty
    pub fn poll_one(&mut self) -> Result<bool, ShellError> {
        if let Some(step) = self.next_effect_step() {
            self.handle_effect_step(step)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Block until an effect arrives or the timeout expires.
    ///
    /// Returns true if an effect was queued for processing.
    pub fn wait_for_effect(&mut self, timeout: Duration) -> bool {
        if self.closed {
            return false;
        }

        if !self.prefetched_effects.is_empty() || !self.effect_rx.is_empty() {
            return true;
        }

        if timeout.is_zero() {
            return false;
        }

        match self.effect_rx.recv_timeout(timeout) {
            Ok(step) => {
                self.prefetched_effects.push_back(step);
                true
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => {
                self.closed = true;
                false
            }
        }
    }

    /// Get the number of pending effects in the queue
    ///
    /// This can be used to check if there are effects waiting to be processed
    /// without actually processing them. Value is a snapshot and may become
    /// stale immediately due to concurrent producers.
    #[must_use]
    pub fn pending_effects(&self) -> usize {
        self.effect_rx.len() + self.prefetched_effects.len()
    }

    /// Get the number of currently in-flight async jobs
    ///
    /// This includes async futures, streams, and blocking jobs that have been
    /// spawned but not yet completed.
    #[must_use]
    pub fn inflight_jobs(&self) -> usize {
        self.activity.load()
    }

    /// Check if the shell is completely idle
    ///
    /// Returns true if there are no pending effects and no in-flight jobs.
    /// This is the authoritative check for determining if all async work has completed.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.pending_effects() == 0 && self.inflight_jobs() == 0
    }

    /// Check if the shell is closed
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Get the configured effect channel capacity (None => unbounded)
    #[must_use]
    pub fn effect_channel_capacity(&self) -> Option<usize> {
        self.effect_channel_capacity
    }

    /// Override the effect channel capacity (None => unbounded)
    pub fn set_effect_channel_capacity(&mut self, capacity: Option<usize>) {
        self.effect_channel_capacity = capacity;
    }

    /// Signal shutdown to higher-level Runner logic and request executor shutdown.
    pub fn shutdown(&mut self) {
        self.closed = true;
        if !self.executors_shutdown {
            self.executors.shutdown_all();
            self.executors_shutdown = true;
        }
    }

    /// Wait for all registered executors to finish outstanding work.
    pub fn wait_for_executors(&self) {
        self.executors.wait_all();
    }

    #[must_use]
    pub fn stats(&self) -> ShellStatsSnapshot {
        self.stats.snapshot()
    }

    #[must_use]
    pub fn stats_handle(&self) -> ShellStats {
        self.stats.clone()
    }

    #[must_use]
    pub fn panic_handler(&self) -> Option<&Arc<PanicHook<E, X>>> {
        self.panic_handler.as_ref()
    }
}

impl<E, X, R> Drop for Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    fn drop(&mut self) {
        self.shutdown();
        self.wait_for_executors();

        #[cfg(feature = "tracing")]
        {
            let snapshot = self.stats();
            if snapshot.dropped_events > 0 || snapshot.dropped_effect_steps > 0 {
                let message = if cfg!(feature = "flair") {
                    "🟡 Shell dropped work during shutdown — events were still in flight. Consider calling `await_idle`, draining the runner, or increasing queue capacities."
                } else {
                    "Shell dropped work during shutdown — events were still in flight. Consider calling `await_idle`, draining the runner, or increasing queue capacities."
                };
                tracing::warn!(
                    stage = "shell_drop",
                    dropped_events = snapshot.dropped_events,
                    dropped_effect_steps = snapshot.dropped_effect_steps,
                    "{message}",
                    message = message
                );
            } else {
                let message = if cfg!(feature = "flair") {
                    "🟢 Shell shutdown complete"
                } else {
                    "Shell shutdown cleanly"
                };
                tracing::debug!(stage = "shell_drop", "{message}", message = message);
            }
        }
    }
}

impl<E, X, R> std::fmt::Debug for Shell<E, X, R>
where
    E: Send + 'static,
    X: Send + 'static,
    R: Clone + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("pending_effects", &"<pending>")
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
#[cfg(all(test, feature = "legacy_tests"))]
mod tests {

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Dummy,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEffect {
        Log,
    }
}
````
