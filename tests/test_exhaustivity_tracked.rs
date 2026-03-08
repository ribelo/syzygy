use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
use syzygy::extract::EventContext;
use syzygy::test_store::{Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Step1,
    Step2,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    MyEffect,
}

fn lease() -> TaskLease {
    static LEASE: OnceLock<TaskLease> = OnceLock::new();
    LEASE.get_or_init(TaskLease::new).clone()
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Step1 => Command::abortable(lease(), Effect::MyEffect),
        Event::Step2 => Command::none(),
    }
}

#[test]
fn tracked_assertion_drains_effect_for_exhaustivity() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
    store.send(Event::Step1);
    store.assert_abortable_effect(lease(), Effect::MyEffect);
    store.send(Event::Step2);
}

#[test]
fn assert_effects_rejects_tracked_effects() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::Off);
    store.send(Event::Step1);

    let message = syzygy::test_store::assert_panic(|| {
        store.assert_effects([Effect::MyEffect]);
    });
    assert!(message.contains("cannot assert plain effects while abortable effects are pending"));
}
