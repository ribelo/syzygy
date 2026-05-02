//! Synchronous test harness for event handler logic.
//!
//! `TestStore` mirrors the event->state->effect loop in-process, without running
//! real async effects. Use it to assert state transitions and emitted effects,
//! then simulate effect completion by feeding follow-up events back into the store.

use std::any::Any;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};

#[cfg(feature = "shell")]
use futures::StreamExt;

use crate::command::{Command, CommandStep, TaskLease};
use crate::core::Core;
#[cfg(feature = "shell")]
use crate::executor::{BlockingCancelToken, Task};
#[cfg(feature = "shell")]
use crate::extract::EffectContext;
use crate::extract::EventContext;
use crate::resource::ResourceMap;

/// Default upper bound for event handler steps executed by [`TestStore::send`].
///
/// Guards tests against accidental infinite event loops.
const DEFAULT_MAX_EVENT_STEPS: usize = 10_000;

/// Hard cap for stream commands consumed by [`TestStore::receive`].
///
/// Keeps tests from deadlocking when a stream is intentionally or accidentally unbounded.
#[cfg(feature = "shell")]
const MAX_RECEIVE_STREAM_COMMANDS: usize = 10_000;

/// Controls whether [`TestStore`] enforces effect assertions between sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exhaustivity {
    On,
    Off,
}

#[derive(Debug)]
struct BufferedEffect<X> {
    lease: Option<TaskLease>,
    effect: X,
}

/// TCA-style synchronous harness for testing event logic.
///
/// The store processes events synchronously, mutates model state through your
/// event handler, and buffers emitted effects for assertions.
pub struct TestStore<E, X, M>
where
    E: 'static,
    X: 'static,
{
    core: Core<E, X, M>,
    pending_events: VecDeque<E>,
    pending_effects: Vec<BufferedEffect<X>>,
    cancelled_leases: Vec<TaskLease>,
    max_event_steps: usize,
    exhaustivity: Exhaustivity,
    effects_asserted: bool,
    resources: ResourceMap,
}

impl<E, X, M> TestStore<E, X, M>
where
    E: 'static,
    X: 'static,
{
    /// Create a new test store from model state and event handler.
    pub fn new<H>(model: M, handler: H) -> Self
    where
        H: Fn(E, &EventContext<M>) -> Command<E, X> + 'static,
    {
        let (core, _sender) = Core::new(Box::new(handler), model);
        Self {
            core,
            pending_events: VecDeque::new(),
            pending_effects: Vec::new(),
            cancelled_leases: Vec::new(),
            max_event_steps: DEFAULT_MAX_EVENT_STEPS,
            exhaustivity: Exhaustivity::Off,
            effects_asserted: true,
            resources: ResourceMap::new(),
        }
    }

    /// Override the event-step safety bound used by [`send`](Self::send).
    #[must_use]
    pub fn with_max_event_steps(mut self, max_event_steps: usize) -> Self {
        assert!(
            max_event_steps > 0,
            "max_event_steps must be greater than zero"
        );
        self.max_event_steps = max_event_steps;
        self
    }

    /// Configure whether effect assertions are required between sends.
    #[must_use]
    pub fn with_exhaustivity(mut self, exhaustivity: Exhaustivity) -> Self {
        self.exhaustivity = exhaustivity;
        self
    }

    #[must_use]
    pub fn with_resource<T: Clone + 'static>(mut self, resource: T) -> Self {
        self.resources.insert(resource);
        self
    }

    /// Send one external event step through the core.
    ///
    /// Events emitted by returned commands are enqueued and processed by the
    /// next [`send`](Self::send)/`receive` cycle, mirroring runner step
    /// semantics.
    ///
    /// Any emitted effects are buffered until asserted or drained with
    /// [`take_effects`](Self::take_effects).
    pub fn send(&mut self, event: E) {
        let send_allowed = self.exhaustivity == Exhaustivity::Off
            || self.effects_asserted
            || (self.pending_effects.is_empty() && self.cancelled_leases.is_empty());
        assert!(
            send_allowed,
            "must assert effects before sending next event. {} unasserted outputs pending ({} effects, {} cancellations).",
            self.pending_effects.len() + self.cancelled_leases.len(),
            self.pending_effects.len(),
            self.cancelled_leases.len(),
        );

        self.pending_events.push_back(event);

        let step_budget = self.pending_events.len();
        let mut processed = 0usize;
        let mut touched_outputs = false;
        for _ in 0..step_budget {
            let Some(next) = self.pending_events.pop_front() else {
                break;
            };
            processed += 1;
            assert!(
                processed <= self.max_event_steps,
                "event processing exceeded max_event_steps={} (likely infinite loop)",
                self.max_event_steps
            );

            let command = self.core.handle_event(next);
            touched_outputs |= self.route_command(command);
        }

        self.effects_asserted = !touched_outputs
            || (self.pending_effects.is_empty() && self.cancelled_leases.is_empty());
    }

    pub fn send_assert(&mut self, event: E, update_expected: impl FnOnce(&mut M)) -> &mut Self
    where
        M: Clone + PartialEq + std::fmt::Debug,
    {
        let mut expected = self.state().clone();
        update_expected(&mut expected);
        self.send(event);
        self.assert_state(&expected);
        self
    }

    /// Returns the current model state.
    #[must_use]
    pub fn state(&self) -> &M {
        self.core.model()
    }

    /// Returns mutable access to model state.
    pub fn state_mut(&mut self) -> &mut M {
        self.core.model_mut()
    }

    /// Assert that the current state equals `expected`.
    pub fn assert_state(&self, expected: &M)
    where
        M: PartialEq + std::fmt::Debug,
    {
        assert_eq!(self.state(), expected, "unexpected state");
    }

    /// Run custom assertions against the current state.
    pub fn assert_state_changed(&self, check: impl FnOnce(&M)) {
        check(self.state());
    }

    /// Number of buffered effects waiting for assertion.
    #[must_use]
    pub fn pending_effect_count(&self) -> usize {
        self.pending_effects.len()
    }

    /// Returns true when there are no pending buffered effects.
    #[must_use]
    pub fn has_no_effects(&self) -> bool {
        self.pending_effects.is_empty()
    }

    /// Drains and returns all buffered effects in emission order.
    pub fn take_effects(&mut self) -> Vec<X> {
        let abortable_leases = self
            .pending_effects
            .iter()
            .filter_map(|buffered| buffered.lease.clone())
            .collect::<Vec<_>>();
        assert!(
            abortable_leases.is_empty(),
            "cannot take plain effects while abortable effects are pending in leases {abortable_leases:?}; use assert_abortable_effect()"
        );

        let effects = self
            .pending_effects
            .drain(..)
            .map(|buffered| buffered.effect)
            .collect();
        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_leases.is_empty();
        effects
    }

    /// Assert that buffered effects exactly match `expected` and drain them.
    pub fn assert_effects<I>(&mut self, expected: I)
    where
        I: IntoIterator<Item = X>,
        X: std::fmt::Debug + PartialEq,
    {
        let abortable_leases = self
            .pending_effects
            .iter()
            .filter_map(|buffered| buffered.lease.clone())
            .collect::<Vec<_>>();
        assert!(
            abortable_leases.is_empty(),
            "cannot assert plain effects while abortable effects are pending in leases {abortable_leases:?}; use assert_abortable_effect()"
        );

        let expected: Vec<X> = expected.into_iter().collect();
        let actual = self.take_effects();
        assert_eq!(actual, expected, "unexpected emitted effects");
        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_leases.is_empty();
    }

    /// Assert and consume a single abortable effect in this test step.
    pub fn assert_abortable_effect(&mut self, lease: impl Into<TaskLease>, expected: X) -> X
    where
        X: std::fmt::Debug + PartialEq,
    {
        let lease = lease.into();
        let Some(index) = self
            .pending_effects
            .iter()
            .position(|buffered| buffered.lease.as_ref() == Some(&lease))
        else {
            panic!(
                "expected abortable effect in lease {lease:?}, but pending abortable leases were {:?}",
                self.pending_effects
                    .iter()
                    .filter_map(|buffered| buffered.lease.clone())
                    .collect::<Vec<_>>()
            );
        };

        let buffered = self.pending_effects.remove(index);
        assert_eq!(
            buffered.effect, expected,
            "unexpected abortable effect for lease {lease:?}"
        );

        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_leases.is_empty();
        buffered.effect
    }

    /// Assert and consume the next cancellation lease.
    pub fn assert_cancelled(&mut self, lease: impl Into<TaskLease>) {
        let lease = lease.into();
        let Some(actual) = self.cancelled_leases.first().cloned() else {
            panic!("expected lease {lease:?} to be cancelled, but no cancellations were pending");
        };

        assert_eq!(
            actual, lease,
            "expected next cancelled lease to be {lease:?}, got {actual:?}"
        );
        self.cancelled_leases.remove(0);
        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_leases.is_empty();
    }

    /// Assert that an abortable lease has no pending effect.
    pub fn assert_lease_empty(&self, lease: impl Into<TaskLease>) {
        let lease = lease.into();
        assert!(
            !self
                .pending_effects
                .iter()
                .any(|buffered| buffered.lease.as_ref() == Some(&lease)),
            "expected lease {lease:?} to be empty"
        );
    }

    /// Assert that no effects are currently buffered.
    pub fn assert_no_effects(&mut self)
    where
        X: std::fmt::Debug,
    {
        assert!(
            self.pending_effects.is_empty() && self.cancelled_leases.is_empty(),
            "expected no emitted effects/cancellations, got effects {:?} and cancelled leases {:?}",
            self.pending_effects
                .iter()
                .map(|buffered| &buffered.effect)
                .collect::<Vec<_>>(),
            self.cancelled_leases
        );
        self.effects_asserted = true;
    }

    /// Drain pending effects, execute them through `effect_handler`, and feed
    /// returned events back through [`send`](Self::send).
    #[cfg(feature = "shell")]
    pub fn receive<H>(&mut self, effect_handler: H)
    where
        H: Fn(X, &EffectContext<'_>) -> Task<E, X>,
    {
        for effect in self.take_effects() {
            let ctx = EffectContext::new(&self.resources);
            let task = effect_handler(effect, &ctx);
            self.drive_received_task(task);
        }

        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_leases.is_empty();
    }

    /// Async variant of [`receive`](Self::receive) for async test contexts.
    ///
    /// This method keeps TestStore's contract explicit:
    /// - It is still a synchronous-style test helper for driving effect results.
    /// - It does **not** integrate with the caller's runtime/executor scheduling.
    /// - It evaluates returned tasks via TestStore's internal `Runtime::block_on`
    ///   path, just like [`receive`](Self::receive).
    /// - It intentionally rejects shell-owned process tasks (`Task::Process`);
    ///   use [`RunnerTester`](crate::runner_tester::RunnerTester) for real shell/runtime lifecycle tests.
    #[cfg(feature = "shell")]
    pub async fn receive_async<H>(&mut self, effect_handler: H)
    where
        H: Fn(X, &EffectContext<'_>) -> Task<E, X>,
    {
        // Preserve the async API for callers already running inside an async test context.
        futures::future::ready(()).await;

        for effect in self.take_effects() {
            let ctx = EffectContext::new(&self.resources);
            let task = effect_handler(effect, &ctx);
            self.drive_received_task_async(task);
        }

        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_leases.is_empty();
    }

    #[cfg(feature = "shell")]
    fn drive_received_task(&mut self, task: Task<E, X>) {
        match task {
            Task::None => {}
            Task::Resolved(command) => {
                self.feed_received_command(command);
            }
            Task::Future(future) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let command = runtime.block_on(future);
                self.feed_received_command(command);
            }
            Task::Stream(stream) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let commands = runtime.block_on(async {
                    stream
                        .take(MAX_RECEIVE_STREAM_COMMANDS + 1)
                        .collect::<Vec<_>>()
                        .await
                });
                assert!(
                    commands.len() <= MAX_RECEIVE_STREAM_COMMANDS,
                    "TestStore::receive reached stream command limit ({MAX_RECEIVE_STREAM_COMMANDS}). This usually means the stream is unbounded; use a finite stream in tests."
                );

                for command in commands {
                    self.feed_received_command(command);
                }
            }
            Task::Process(_task) => {
                panic!(
                    "TestStore::receive does not support process tasks; use a real Syzygy runner for shell-owned process lifecycle tests"
                );
            }
            Task::Blocking(task) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let command = runtime.block_on(task.into_future(runtime.clone()));
                self.feed_received_command(command);
            }
            Task::BlockingCooperative(task) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let command =
                    runtime.block_on(task.into_future(runtime.clone(), BlockingCancelToken::new()));
                if let Some(command) = command {
                    self.feed_received_command(command);
                }
            }
        }
    }

    #[cfg(feature = "shell")]
    fn drive_received_task_async(&mut self, task: Task<E, X>) {
        match task {
            Task::None => {}
            Task::Resolved(command) => {
                self.feed_received_command(command);
            }
            Task::Future(future) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let command = runtime.block_on(future);
                self.feed_received_command(command);
            }
            Task::Stream(stream) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let commands = runtime.block_on(async {
                    stream
                        .take(MAX_RECEIVE_STREAM_COMMANDS + 1)
                        .collect::<Vec<_>>()
                        .await
                });
                assert!(
                    commands.len() <= MAX_RECEIVE_STREAM_COMMANDS,
                    "TestStore::receive reached stream command limit ({MAX_RECEIVE_STREAM_COMMANDS}). This usually means the stream is unbounded; use a finite stream in tests."
                );

                for command in commands {
                    self.feed_received_command(command);
                }
            }
            Task::Process(_task) => {
                panic!(
                    "TestStore::receive_async does not support process tasks; use a real Syzygy runner for shell-owned process lifecycle tests"
                );
            }
            Task::Blocking(task) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let command = runtime.block_on(task.into_future(runtime.clone()));
                self.feed_received_command(command);
            }
            Task::BlockingCooperative(task) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                let command =
                    runtime.block_on(task.into_future(runtime.clone(), BlockingCancelToken::new()));
                if let Some(command) = command {
                    self.feed_received_command(command);
                }
            }
        }
    }

    #[cfg(feature = "shell")]
    fn feed_received_command(&mut self, command: Command<E, X>) {
        for step in command {
            match step {
                CommandStep::Event(event) => {
                    self.send_from_receive(event);
                }
                CommandStep::Effect(effect) => {
                    self.pending_effects.push(BufferedEffect {
                        lease: None,
                        effect,
                    });
                }
                CommandStep::Abortable { lease, effect } => {
                    self.upsert_abortable_effect(lease, effect);
                }
                CommandStep::Cancel { lease } => {
                    self.cancel_abortable_effect(&lease);
                    self.cancelled_leases.push(lease);
                }
                CommandStep::ProcessWrite { .. } | CommandStep::ProcessCloseStdin { .. } => {
                    panic!(
                        "TestStore::receive does not support interactive process control; use a real Syzygy runner"
                    );
                }
            }
        }
    }

    #[cfg(feature = "shell")]
    fn send_from_receive(&mut self, event: E) {
        self.effects_asserted = true;
        self.send(event);
    }

    fn route_command(&mut self, command: Command<E, X>) -> bool {
        let mut touched_outputs = false;
        for step in command {
            match step {
                CommandStep::Event(event) => {
                    self.pending_events.push_back(event);
                }
                CommandStep::Effect(effect) => {
                    touched_outputs = true;
                    self.pending_effects.push(BufferedEffect {
                        lease: None,
                        effect,
                    });
                }
                CommandStep::Abortable { lease, effect } => {
                    touched_outputs = true;
                    self.upsert_abortable_effect(lease, effect);
                }
                CommandStep::Cancel { lease } => {
                    touched_outputs = true;
                    self.cancel_abortable_effect(&lease);
                    self.cancelled_leases.push(lease);
                }
                CommandStep::ProcessWrite { .. } | CommandStep::ProcessCloseStdin { .. } => {
                    panic!(
                        "TestStore does not support interactive process control commands; use a real Syzygy runner"
                    );
                }
            }
        }

        touched_outputs
    }

    fn upsert_abortable_effect(&mut self, lease: TaskLease, effect: X) {
        if let Some(index) = self
            .pending_effects
            .iter()
            .position(|buffered| buffered.lease.as_ref() == Some(&lease))
        {
            self.pending_effects.remove(index);
            self.cancelled_leases.push(lease.clone());
        }

        self.pending_effects.push(BufferedEffect {
            lease: Some(lease),
            effect,
        });
    }

    fn cancel_abortable_effect(&mut self, lease: &TaskLease) {
        if let Some(index) = self
            .pending_effects
            .iter()
            .position(|buffered| buffered.lease.as_ref() == Some(lease))
        {
            self.pending_effects.remove(index);
        }
    }
}

impl<E, X, M> Drop for TestStore<E, X, M>
where
    E: 'static,
    X: 'static,
{
    fn drop(&mut self) {
        if self.exhaustivity == Exhaustivity::Off {
            return;
        }

        if self.effects_asserted
            || (self.pending_effects.is_empty() && self.cancelled_leases.is_empty())
        {
            return;
        }

        let message = format!(
            "must assert effects before dropping test store. {} unasserted outputs pending ({} effects, {} cancellations).",
            self.pending_effects.len() + self.cancelled_leases.len(),
            self.pending_effects.len(),
            self.cancelled_leases.len(),
        );
        eprintln!("{message}");
        assert!(std::thread::panicking(), "{message}");
    }
}

/// Execute `f` and return the panic message.
///
/// Panics if `f` does not panic.
pub fn assert_panic<F>(f: F) -> String
where
    F: FnOnce(),
{
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(()) => panic!("expected panic, but closure completed successfully"),
        Err(payload) => panic_payload_to_string(payload),
    }
}

/// Execute `f` and assert its panic message contains `expected_substring`.
pub fn assert_panic_contains<F>(expected_substring: &str, f: F)
where
    F: FnOnce(),
{
    let message = assert_panic(f);
    assert!(
        message.contains(expected_substring),
        "panic message did not contain '{expected_substring}'. actual: '{message}'"
    );
}

fn panic_payload_to_string(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }

    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }

    "panic payload is not a string".to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;
    use crate as syzygy;
    use crate::command::TaskLease;
    #[cfg(feature = "shell")]
    use crate::executor::Task;
    use crate::extract::EventHandler;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Event {
        Increment(i32),
        Start,
        SaveDone,
        EmitMany,
        DoubleBorrow,
        StartAbortable,
        CancelAbortable,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Effect {
        Log(i32),
        First,
        Second,
        Third,
        Fourth,
        Abortable,
    }

    #[derive(Debug, Default, PartialEq, Eq, crate::Model)]
    struct Model {
        #[model(wrapper = Counter)]
        counter: i32,
        #[model(wrapper = SaveCompleted)]
        save_completed: bool,
    }

    fn abortable_lease() -> TaskLease {
        static LEASE: OnceLock<TaskLease> = OnceLock::new();
        LEASE.get_or_init(TaskLease::new).clone()
    }

    fn increment(amount: i32, counter: &mut Counter) -> Command<Event, Effect> {
        **counter += amount;
        Command::effect(Effect::Log(**counter))
    }

    fn save_done(_: (), done: &mut SaveCompleted) -> Command<Event, Effect> {
        **done = true;
        Command::none()
    }

    fn bad_double_borrow(
        _: (),
        _first: &mut Counter,
        _second: &mut Counter,
    ) -> Command<Event, Effect> {
        Command::none()
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Start => Command::event(Event::Increment(2)),
            Event::SaveDone => save_done.handle((), ctx),
            Event::EmitMany => Command::effect(Effect::First)
                .and_effect(Effect::Second)
                .and_effect(Effect::Third)
                .and_effect(Effect::Fourth),
            Event::DoubleBorrow => bad_double_borrow.handle((), ctx),
            Event::StartAbortable => Command::abortable(abortable_lease(), Effect::Abortable),
            Event::CancelAbortable => Command::cancel(abortable_lease()),
        }
    }

    #[test]
    fn send_updates_state_and_collects_effects() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(5));

        assert_eq!(store.state().counter, 5);
        store.assert_effects([Effect::Log(5)]);
        store.send(Event::SaveDone);
        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[test]
    fn command_events_defer_until_next_send_and_keep_fifo() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Start);

        assert_eq!(store.state().counter, 0);
        store.assert_no_effects();

        store.send(Event::SaveDone);

        assert_eq!(store.state().counter, 2);
        assert!(store.state().save_completed);
        store.assert_effects([Effect::Log(2)]);
    }

    #[test]
    fn deferred_command_events_run_before_later_external_event() {
        #[derive(Debug, Clone)]
        enum Ev {
            Start,
            A,
            B,
            External,
        }

        #[derive(Debug, Clone)]
        enum Fx {}

        #[derive(Debug, Default, PartialEq, Eq, crate::Model)]
        struct OrderModel {
            #[model(wrapper = Order)]
            order: Vec<&'static str>,
        }

        fn on_event(event: Ev, ctx: &EventContext<OrderModel>) -> Command<Ev, Fx> {
            match event {
                Ev::Start => Command::event(Ev::A).and_event(Ev::B),
                Ev::A => {
                    let order = <Order as crate::extract::PartMut<OrderModel>>::extract_mut(ctx);
                    order.push("a");
                    Command::none()
                }
                Ev::B => {
                    let order = <Order as crate::extract::PartMut<OrderModel>>::extract_mut(ctx);
                    order.push("b");
                    Command::none()
                }
                Ev::External => {
                    let order = <Order as crate::extract::PartMut<OrderModel>>::extract_mut(ctx);
                    order.push("x");
                    Command::none()
                }
            }
        }

        let mut store = TestStore::new(OrderModel::default(), on_event);

        store.send(Ev::Start);
        assert!(store.state().order.is_empty());

        store.send(Ev::External);
        assert_eq!(store.state().order, ["a", "b", "x"]);
    }

    #[test]
    fn collects_multiple_effect_steps() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::EmitMany);

        store.assert_effects([Effect::First, Effect::Second, Effect::Third, Effect::Fourth]);
    }

    #[test]
    fn assert_panic_helper_captures_borrow_rule_panics() {
        let mut store = TestStore::new(Model::default(), handle_event);

        assert_panic_contains("already borrowed mutably", || {
            store.send(Event::DoubleBorrow);
        });
    }

    #[test]
    fn exhaustive_mode_panics_on_unasserted_effects() {
        assert_panic_contains("must assert effects before sending next event", || {
            let mut store =
                TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::On);

            store.send(Event::Increment(1));
            store.send(Event::Increment(1));
        });
    }

    #[test]
    fn exhaustive_mode_allows_send_after_assert_effects() {
        let mut store =
            TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::On);

        store.send(Event::Increment(1));
        store.assert_effects([Effect::Log(1)]);
        store.send(Event::Increment(2));

        store.assert_effects([Effect::Log(3)]);
    }

    #[test]
    fn exhaustive_mode_allows_send_when_no_effects() {
        let mut store =
            TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::On);

        store.send(Event::SaveDone);
        store.send(Event::SaveDone);

        store.assert_no_effects();
    }

    #[test]
    fn exhaustive_mode_panics_on_drop_with_unasserted_effects() {
        assert_panic_contains("must assert effects before dropping test store", || {
            let mut store =
                TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::On);
            store.send(Event::Increment(1));
        });
    }

    #[test]
    fn non_exhaustive_mode_allows_unasserted_effects() {
        let mut store =
            TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::Off);

        store.send(Event::Increment(1));
        store.send(Event::Increment(1));

        assert_eq!(store.pending_effect_count(), 2);
    }

    #[test]
    fn abortable_effect_assertion_consumes_lease() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::StartAbortable);
        store.assert_abortable_effect(abortable_lease(), Effect::Abortable);

        store.assert_no_effects();
    }

    #[test]
    fn assert_effects_rejects_abortable_outputs() {
        let mut store = TestStore::new(Model::default(), handle_event);
        store.send(Event::StartAbortable);

        assert_panic_contains(
            "cannot assert plain effects while abortable effects are pending",
            || {
                store.assert_effects([Effect::Abortable]);
            },
        );
    }

    #[test]
    fn abortable_overwrite_records_cancellation() {
        let mut store = TestStore::new(Model::default(), |_event: Event, _ctx| {
            let lease = abortable_lease();
            Command::abortable(&lease, Effect::First).and_abortable(&lease, Effect::Second)
        });

        store.send(Event::StartAbortable);
        store.assert_cancelled(abortable_lease());
        store.assert_abortable_effect(abortable_lease(), Effect::Second);
        store.assert_lease_empty(abortable_lease());
    }

    #[test]
    fn cancelled_lease_requires_assertion_in_exhaustive_mode() {
        assert_panic_contains("must assert effects", || {
            let mut store =
                TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::On);
            store.send(Event::CancelAbortable);
        });
    }

    #[test]
    fn assert_cancelled_consumes_pending_cancellation() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::CancelAbortable);
        store.assert_cancelled(abortable_lease());
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_feeds_resolved_task_events_back() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(1));
        store.receive(|effect, _ctx| match effect {
            Effect::Log(_) => Task::resolved(Command::event(Event::SaveDone)),
            _ => Task::none(),
        });

        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_supports_runtime_sleep_in_future_tasks() {
        use std::time::Duration;

        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(1));
        store.receive(|effect, _ctx| match effect {
            Effect::Log(_) => Task::once(async {
                crate::runtime::sleep(Duration::from_millis(1)).await;
                Command::event(Event::SaveDone)
            }),
            _ => Task::none(),
        });

        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_feeds_blocking_task_events_back() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(1));
        store.receive(|effect, _ctx| match effect {
            Effect::Log(_) => Task::blocking(|| Command::event(Event::SaveDone)),
            _ => Task::none(),
        });

        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_works_with_exhaustive_mode() {
        let mut store =
            TestStore::new(Model::default(), handle_event).with_exhaustivity(Exhaustivity::On);

        store.send(Event::Increment(1));
        store.receive(|_effect, _ctx| Task::none());
        store.send(Event::SaveDone);

        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_handles_task_none() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(2));
        store.receive(|_effect, _ctx| Task::none());

        assert_eq!(store.state().counter, 2);
        assert!(!store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_cascading_effects_stay_pending() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(1));
        store.receive(|_effect, _ctx| Task::resolved(Command::event(Event::Increment(2))));

        assert_eq!(store.state().counter, 3);
        assert_eq!(store.pending_effect_count(), 1);
        store.assert_effects([Effect::Log(3)]);
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_panics_for_unbounded_streams() {
        use futures::stream;

        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(1));
        assert_panic_contains("stream command limit", || {
            store.receive(|_effect, _ctx| Task::stream(stream::repeat(Command::none())));
        });
    }

    #[test]
    fn take_effects_rejects_abortable_outputs() {
        let mut store = TestStore::new(Model::default(), |_event: Event, _ctx| {
            Command::<Event, Effect>::abortable(abortable_lease(), Effect::First)
        });

        store.send(Event::StartAbortable);
        assert_panic_contains("abortable effects are pending", || {
            let _ = store.take_effects();
        });
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_async_runs_inside_async_context() {
        futures::executor::block_on(async {
            let mut store = TestStore::new(Model::default(), handle_event);
            store.send(Event::Increment(1));

            store
                .receive_async(|effect, _ctx| match effect {
                    Effect::Log(_) => Task::once(async { Command::event(Event::SaveDone) }),
                    _ => Task::none(),
                })
                .await;

            assert!(store.state().save_completed);
        });
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_async_supports_runtime_sleep_in_stream_tasks() {
        use std::time::Duration;

        futures::executor::block_on(async {
            let mut store = TestStore::new(Model::default(), handle_event);
            store.send(Event::Increment(1));

            store
                .receive_async(|effect, _ctx| match effect {
                    Effect::Log(_) => Task::stream(futures::stream::once(async {
                        crate::runtime::sleep(Duration::from_millis(1)).await;
                        Command::event(Event::SaveDone)
                    })),
                    _ => Task::none(),
                })
                .await;

            assert!(store.state().save_completed);
        });
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_async_panics_for_process_tasks() {
        use crate::process::ProcessSpec;

        let mut store = TestStore::new(Model::default(), handle_event);
        store.send(Event::Increment(1));

        assert_panic_contains("receive_async does not support process tasks", || {
            futures::executor::block_on(async {
                store
                    .receive_async(|_effect, _ctx| {
                        Task::process(ProcessSpec::new("echo"), |_result| Command::none())
                    })
                    .await;
            });
        });
    }

    #[test]
    fn assert_state_checks_equality() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(3));

        store.assert_state(&Model {
            counter: 3,
            save_completed: false,
        });
    }

    #[test]
    fn assert_state_changed_runs_closure() {
        let mut store = TestStore::new(Model::default(), handle_event);

        store.send(Event::Increment(4));

        store.assert_state_changed(|state| {
            assert_eq!(state.counter, 4);
            assert!(!state.save_completed);
        });
    }
}
