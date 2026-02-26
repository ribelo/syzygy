//! Synchronous test harness for reducer logic.
//!
//! `TestStore` mirrors the event->state->effect loop in-process, without running
//! real async effects. Use it to assert state transitions and emitted effects,
//! then simulate effect completion by feeding follow-up events back into the store.

use std::any::Any;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};

#[cfg(feature = "shell")]
use futures::executor::block_on;
#[cfg(feature = "shell")]
use futures::StreamExt;

use crate::command::{CancelId, Command, CommandStep, IntoCancelId};
use crate::core::Core;
use crate::dependency::ResourceMap;
#[cfg(feature = "shell")]
use crate::executor::Task;
#[cfg(feature = "shell")]
use crate::extract::EffectContext;
use crate::extract::EventContext;

/// Default upper bound for reducer steps executed by [`TestStore::send`].
///
/// Guards tests against accidental infinite event loops.
const DEFAULT_MAX_EVENT_STEPS: usize = 10_000;

/// Controls whether [`TestStore`] enforces effect assertions between sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exhaustivity {
    On,
    Off,
}

#[derive(Debug)]
struct BufferedEffect<X> {
    id: Option<CancelId>,
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
    cancelled_slots: Vec<CancelId>,
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
    /// Create a new test store from model state and event dispatcher.
    pub fn new<H>(model: M, handler: H) -> Self
    where
        H: Fn(E, &EventContext<M>) -> Command<E, X> + 'static,
    {
        let (core, _sender) = Core::new(Box::new(handler), model);
        Self {
            core,
            pending_events: VecDeque::new(),
            pending_effects: Vec::new(),
            cancelled_slots: Vec::new(),
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

    #[must_use]
    pub fn with_dependency<T: Clone + 'static>(self, resource: T) -> Self {
        self.with_resource(resource)
    }

    /// Send an event and process all synchronously chained events.
    ///
    /// Any emitted effects are buffered until asserted or drained with
    /// [`take_effects`](Self::take_effects).
    pub fn send(&mut self, event: E) {
        let send_allowed = self.exhaustivity == Exhaustivity::Off
            || self.effects_asserted
            || (self.pending_effects.is_empty() && self.cancelled_slots.is_empty());
        assert!(
            send_allowed,
            "must assert effects before sending next event. {} unasserted outputs pending ({} effects, {} cancellations).",
            self.pending_effects.len() + self.cancelled_slots.len(),
            self.pending_effects.len(),
            self.cancelled_slots.len(),
        );

        self.pending_events.push_back(event);

        let mut processed = 0usize;
        let mut touched_outputs = false;
        while let Some(next) = self.pending_events.pop_front() {
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
            || (self.pending_effects.is_empty() && self.cancelled_slots.is_empty());
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
        let effects = self
            .pending_effects
            .drain(..)
            .map(|buffered| buffered.effect)
            .collect();
        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_slots.is_empty();
        effects
    }

    /// Assert that buffered effects exactly match `expected` and drain them.
    pub fn assert_effects<I>(&mut self, expected: I)
    where
        I: IntoIterator<Item = X>,
        X: std::fmt::Debug + PartialEq,
    {
        let tracked_slots = self
            .pending_effects
            .iter()
            .filter_map(|buffered| buffered.id)
            .collect::<Vec<_>>();
        assert!(
            tracked_slots.is_empty(),
            "cannot assert plain effects while tracked effects are pending in slots {tracked_slots:?}; use assert_tracked_effect()"
        );

        let expected: Vec<X> = expected.into_iter().collect();
        let actual = self.take_effects();
        assert_eq!(actual, expected, "unexpected emitted effects");
        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_slots.is_empty();
    }

    /// Assert and consume a single tracked effect in this test step.
    pub fn assert_tracked_effect(&mut self, id: impl IntoCancelId, expected: X) -> X
    where
        X: std::fmt::Debug + PartialEq,
    {
        let id = id.into_cancel_id();
        let Some(index) = self
            .pending_effects
            .iter()
            .position(|buffered| buffered.id == Some(id))
        else {
            panic!(
                "expected tracked effect in slot {id:?}, but pending tracked slots were {:?}",
                self.pending_effects
                    .iter()
                    .filter_map(|buffered| buffered.id)
                    .collect::<Vec<_>>()
            );
        };

        let buffered = self.pending_effects.remove(index);
        assert_eq!(
            buffered.effect, expected,
            "unexpected tracked effect for slot {id:?}"
        );

        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_slots.is_empty();
        buffered.effect
    }

    /// Assert and consume the next cancellation slot.
    pub fn assert_cancelled(&mut self, id: impl IntoCancelId) {
        let id = id.into_cancel_id();
        let Some(actual) = self.cancelled_slots.first().copied() else {
            panic!("expected slot {id:?} to be cancelled, but no cancellations were pending");
        };

        assert_eq!(
            actual, id,
            "expected next cancelled slot to be {id:?}, got {actual:?}"
        );
        self.cancelled_slots.remove(0);
        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_slots.is_empty();
    }

    /// Assert that a tracked slot has no pending effect.
    pub fn assert_slot_empty(&self, id: impl IntoCancelId) {
        let id = id.into_cancel_id();
        assert!(
            !self
                .pending_effects
                .iter()
                .any(|buffered| buffered.id == Some(id)),
            "expected slot {id:?} to be empty"
        );
    }

    /// Assert that no effects are currently buffered.
    pub fn assert_no_effects(&mut self)
    where
        X: std::fmt::Debug,
    {
        assert!(
            self.pending_effects.is_empty() && self.cancelled_slots.is_empty(),
            "expected no emitted effects/cancellations, got effects {:?} and cancelled slots {:?}",
            self.pending_effects
                .iter()
                .map(|buffered| &buffered.effect)
                .collect::<Vec<_>>(),
            self.cancelled_slots
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

        self.effects_asserted = self.pending_effects.is_empty() && self.cancelled_slots.is_empty();
    }

    #[cfg(feature = "shell")]
    fn drive_received_task(&mut self, task: Task<E, X>) {
        match task {
            Task::None => {}
            Task::Resolved(command) => {
                self.feed_received_command(command);
            }
            Task::Future(future) => {
                let command = block_on(future);
                self.feed_received_command(command);
            }
            Task::Stream(stream) => {
                let commands = block_on(stream.collect::<Vec<_>>());
                for command in commands {
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
                    self.pending_effects
                        .push(BufferedEffect { id: None, effect });
                }
                CommandStep::Tracked { id, effect } => {
                    self.upsert_tracked_effect(id, effect);
                }
                CommandStep::Cancel { id } => {
                    self.cancel_tracked_effect(id);
                    self.cancelled_slots.push(id);
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
                    self.pending_effects
                        .push(BufferedEffect { id: None, effect });
                }
                CommandStep::Tracked { id, effect } => {
                    touched_outputs = true;
                    self.upsert_tracked_effect(id, effect);
                }
                CommandStep::Cancel { id } => {
                    touched_outputs = true;
                    self.cancel_tracked_effect(id);
                    self.cancelled_slots.push(id);
                }
            }
        }

        touched_outputs
    }

    fn upsert_tracked_effect(&mut self, id: CancelId, effect: X) {
        if let Some(index) = self
            .pending_effects
            .iter()
            .position(|buffered| buffered.id == Some(id))
        {
            self.pending_effects.remove(index);
            self.cancelled_slots.push(id);
        }

        self.pending_effects.push(BufferedEffect {
            id: Some(id),
            effect,
        });
    }

    fn cancel_tracked_effect(&mut self, id: CancelId) {
        if let Some(index) = self
            .pending_effects
            .iter()
            .position(|buffered| buffered.id == Some(id))
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
            || (self.pending_effects.is_empty() && self.cancelled_slots.is_empty())
        {
            return;
        }

        let message = format!(
            "must assert effects before dropping test store. {} unasserted outputs pending ({} effects, {} cancellations).",
            self.pending_effects.len() + self.cancelled_slots.len(),
            self.pending_effects.len(),
            self.cancelled_slots.len(),
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
    use super::*;
    use crate as syzygy;
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
        Track,
        CancelTracked,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Effect {
        Log(i32),
        First,
        Second,
        Third,
        Fourth,
        Tracked,
    }

    #[derive(Debug, Default, PartialEq, Eq, crate::Model)]
    struct Model {
        counter: i32,
        save_completed: bool,
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

    fn dispatch(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Start => Command::event(Event::Increment(2)),
            Event::SaveDone => save_done.handle((), ctx),
            Event::EmitMany => Command::effect(Effect::First)
                .and_effect(Effect::Second)
                .and_effect(Effect::Third)
                .and_effect(Effect::Fourth),
            Event::DoubleBorrow => bad_double_borrow.handle((), ctx),
            Event::Track => Command::track(1_u8, Effect::Tracked),
            Event::CancelTracked => Command::cancel(1_u8),
        }
    }

    #[test]
    fn send_updates_state_and_collects_effects() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Increment(5));

        assert_eq!(store.state().counter, 5);
        store.assert_effects([Effect::Log(5)]);
        store.send(Event::SaveDone);
        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[test]
    fn send_processes_synchronous_event_chains() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Start);

        assert_eq!(store.state().counter, 2);
        store.assert_effects([Effect::Log(2)]);
    }

    #[test]
    fn collects_multiple_effect_steps() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::EmitMany);

        store.assert_effects([Effect::First, Effect::Second, Effect::Third, Effect::Fourth]);
    }

    #[test]
    fn assert_panic_helper_captures_borrow_rule_panics() {
        let mut store = TestStore::new(Model::default(), dispatch);

        assert_panic_contains("already borrowed mutably", || {
            store.send(Event::DoubleBorrow);
        });
    }

    #[test]
    fn exhaustive_mode_panics_on_unasserted_effects() {
        assert_panic_contains("must assert effects before sending next event", || {
            let mut store =
                TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::On);

            store.send(Event::Increment(1));
            store.send(Event::Increment(1));
        });
    }

    #[test]
    fn exhaustive_mode_allows_send_after_assert_effects() {
        let mut store =
            TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::On);

        store.send(Event::Increment(1));
        store.assert_effects([Effect::Log(1)]);
        store.send(Event::Increment(2));

        store.assert_effects([Effect::Log(3)]);
    }

    #[test]
    fn exhaustive_mode_allows_send_when_no_effects() {
        let mut store =
            TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::On);

        store.send(Event::SaveDone);
        store.send(Event::SaveDone);

        store.assert_no_effects();
    }

    #[test]
    fn exhaustive_mode_panics_on_drop_with_unasserted_effects() {
        assert_panic_contains("must assert effects before dropping test store", || {
            let mut store =
                TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::On);
            store.send(Event::Increment(1));
        });
    }

    #[test]
    fn non_exhaustive_mode_allows_unasserted_effects() {
        let mut store =
            TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::Off);

        store.send(Event::Increment(1));
        store.send(Event::Increment(1));

        assert_eq!(store.pending_effect_count(), 2);
    }

    #[test]
    fn tracked_effect_assertion_consumes_slot() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Track);
        store.assert_tracked_effect(1_u8, Effect::Tracked);

        store.assert_no_effects();
    }

    #[test]
    fn assert_effects_rejects_tracked_outputs() {
        let mut store = TestStore::new(Model::default(), dispatch);
        store.send(Event::Track);

        assert_panic_contains(
            "cannot assert plain effects while tracked effects are pending",
            || {
                store.assert_effects([Effect::Tracked]);
            },
        );
    }

    #[test]
    fn tracked_overwrite_records_cancellation() {
        let mut store = TestStore::new(Model::default(), |_event: Event, _ctx| {
            Command::track(1_u8, Effect::First).and_track(1_u8, Effect::Second)
        });

        store.send(Event::Track);
        store.assert_cancelled(1_u8);
        store.assert_tracked_effect(1_u8, Effect::Second);
        store.assert_slot_empty(1_u8);
    }

    #[test]
    fn cancelled_slot_requires_assertion_in_exhaustive_mode() {
        assert_panic_contains("must assert effects", || {
            let mut store =
                TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::On);
            store.send(Event::CancelTracked);
        });
    }

    #[test]
    fn assert_cancelled_consumes_pending_cancellation() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::CancelTracked);
        store.assert_cancelled(1_u8);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_feeds_resolved_task_events_back() {
        let mut store = TestStore::new(Model::default(), dispatch);

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
    fn receive_feeds_blocking_task_events_back() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Increment(1));
        store.receive(|effect, _ctx| match effect {
            Effect::Log(_) => Task::once(async { Command::event(Event::SaveDone) }),
            _ => Task::none(),
        });

        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_works_with_exhaustive_mode() {
        let mut store =
            TestStore::new(Model::default(), dispatch).with_exhaustivity(Exhaustivity::On);

        store.send(Event::Increment(1));
        store.receive(|_effect, _ctx| Task::none());
        store.send(Event::SaveDone);

        assert!(store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_handles_task_none() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Increment(2));
        store.receive(|_effect, _ctx| Task::none());

        assert_eq!(store.state().counter, 2);
        assert!(!store.state().save_completed);
        store.assert_no_effects();
    }

    #[cfg(feature = "shell")]
    #[test]
    fn receive_cascading_effects_stay_pending() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Increment(1));
        store.receive(|_effect, _ctx| Task::resolved(Command::event(Event::Increment(2))));

        assert_eq!(store.state().counter, 3);
        assert_eq!(store.pending_effect_count(), 1);
        store.assert_effects([Effect::Log(3)]);
    }

    #[test]
    fn assert_state_checks_equality() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Increment(3));

        store.assert_state(&Model {
            counter: 3,
            save_completed: false,
        });
    }

    #[test]
    fn assert_state_changed_runs_closure() {
        let mut store = TestStore::new(Model::default(), dispatch);

        store.send(Event::Increment(4));

        store.assert_state_changed(|state| {
            assert_eq!(state.counter, 4);
            assert!(!state.save_completed);
        });
    }
}
