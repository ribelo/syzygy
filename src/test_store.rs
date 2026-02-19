//! Synchronous test harness for reducer logic.
//!
//! `TestStore` mirrors the event->state->effect loop in-process, without running
//! real async effects. Use it to assert state transitions and emitted effects,
//! then simulate effect completion by feeding follow-up events back into the store.

use std::any::Any;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};

use crate::command::{Command, CommandStep};
use crate::core::Core;
use crate::extract::EventContext;

/// Default upper bound for reducer steps executed by [`TestStore::send`].
///
/// Guards tests against accidental infinite event loops.
const DEFAULT_MAX_EVENT_STEPS: usize = 10_000;

/// TCA-style synchronous harness for testing event logic.
///
/// The store processes events synchronously, mutates model state through your
/// event handler, and buffers emitted effects for assertions.
pub struct TestStore<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    core: Core<E, X, M>,
    pending_events: VecDeque<E>,
    pending_effects: Vec<X>,
    max_event_steps: usize,
}

impl<E, X, M> TestStore<E, X, M>
where
    E: Send + 'static,
    X: Send + 'static,
{
    /// Create a new test store from model state and event dispatcher.
    pub fn new<H>(model: M, handler: H) -> Self
    where
        H: Fn(E, &EventContext<M>) -> Command<E, X> + Send + 'static,
    {
        let (core, _sender) = Core::new(Box::new(handler), model);
        Self {
            core,
            pending_events: VecDeque::new(),
            pending_effects: Vec::new(),
            max_event_steps: DEFAULT_MAX_EVENT_STEPS,
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

    /// Send an event and process all synchronously chained events.
    ///
    /// Any emitted effects are buffered until asserted or drained with
    /// [`take_effects`](Self::take_effects).
    pub fn send(&mut self, event: E) {
        self.pending_events.push_back(event);

        let mut processed = 0usize;
        while let Some(next) = self.pending_events.pop_front() {
            processed += 1;
            assert!(
                processed <= self.max_event_steps,
                "event processing exceeded max_event_steps={} (likely infinite loop)",
                self.max_event_steps
            );

            let command = self.core.handle_event(next);
            self.route_command(command);
        }
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
        std::mem::take(&mut self.pending_effects)
    }

    /// Assert that buffered effects exactly match `expected` and drain them.
    pub fn assert_effects<I>(&mut self, expected: I)
    where
        I: IntoIterator<Item = X>,
        X: std::fmt::Debug + PartialEq,
    {
        let expected: Vec<X> = expected.into_iter().collect();
        let actual = self.take_effects();
        assert_eq!(actual, expected, "unexpected emitted effects");
    }

    /// Assert that no effects are currently buffered.
    pub fn assert_no_effects(&self)
    where
        X: std::fmt::Debug,
    {
        assert!(
            self.pending_effects.is_empty(),
            "expected no emitted effects, got {:?}",
            self.pending_effects
        );
    }

    fn route_command(&mut self, command: Command<E, X>) {
        for step in command {
            match step {
                CommandStep::Event(event) => {
                    self.pending_events.push_back(event);
                }
                CommandStep::Effect(effect) | CommandStep::CancellableEffect { effect, .. } => {
                    self.pending_effects.push(effect);
                }
                CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                    self.pending_effects.extend(effects);
                }
            }
        }
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
    use std::ops::{Deref, DerefMut};

    use super::*;
    use crate::extract::{EventHandler, FromEventContext};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Event {
        Increment(i32),
        Start,
        SaveDone,
        EmitMany,
        DoubleBorrow,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Effect {
        Log(i32),
        First,
        Second,
        Third,
        Fourth,
    }

    #[derive(Debug, Default)]
    struct Model {
        counter: i32,
        save_completed: bool,
    }

    struct Counter(*mut i32);

    impl Deref for Counter {
        type Target = i32;

        fn deref(&self) -> &Self::Target {
            // SAFETY: FromEventContext guarantees this pointer was created from
            // the live model and points to `Model::counter`.
            unsafe { &*self.0 }
        }
    }

    impl DerefMut for Counter {
        fn deref_mut(&mut self) -> &mut Self::Target {
            // SAFETY: same invariant as above, now mutably borrowed.
            unsafe { &mut *self.0 }
        }
    }

    impl FromEventContext<Model> for Counter {
        fn from_context(ctx: &EventContext<Model>) -> Self {
            ctx.track_borrow(0, "counter");
            // SAFETY: field index 0 is reserved for `counter`; this matches the
            // wrapper's contract and test model layout.
            Counter(unsafe { &mut (*ctx.model_ptr()).counter })
        }
    }

    struct SaveCompleted(*mut bool);

    impl Deref for SaveCompleted {
        type Target = bool;

        fn deref(&self) -> &Self::Target {
            // SAFETY: pointer targets `Model::save_completed` in the active model.
            unsafe { &*self.0 }
        }
    }

    impl DerefMut for SaveCompleted {
        fn deref_mut(&mut self) -> &mut Self::Target {
            // SAFETY: same invariant as above with mutable access.
            unsafe { &mut *self.0 }
        }
    }

    impl FromEventContext<Model> for SaveCompleted {
        fn from_context(ctx: &EventContext<Model>) -> Self {
            ctx.track_borrow(1, "save_completed");
            // SAFETY: field index 1 is reserved for `save_completed`.
            SaveCompleted(unsafe { &mut (*ctx.model_ptr()).save_completed })
        }
    }

    fn increment(amount: i32, mut counter: Counter) -> Command<Event, Effect> {
        *counter += amount;
        Command::effect(Effect::Log(*counter))
    }

    fn save_done(_: (), mut done: SaveCompleted) -> Command<Event, Effect> {
        *done = true;
        Command::none()
    }

    fn bad_double_borrow(_: (), _first: Counter, _second: Counter) -> Command<Event, Effect> {
        Command::none()
    }

    fn dispatch(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Start => Command::event(Event::Increment(2)),
            Event::SaveDone => save_done.handle((), ctx),
            Event::EmitMany => Command::effect(Effect::First)
                .and(Command::sequential([Effect::Second, Effect::Third]))
                .and(Command::parallel([Effect::Fourth])),
            Event::DoubleBorrow => bad_double_borrow.handle((), ctx),
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
    fn collects_batch_and_parallel_effects() {
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
}
