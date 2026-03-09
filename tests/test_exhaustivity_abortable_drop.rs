use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
use syzygy::extract::EventContext;
use syzygy::test_store::{Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Step1,
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
    }
}

#[test]
fn abortable_assertion_prevents_drop_panic_when_lease_is_drained() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
    store.send(Event::Step1);
    store.assert_abortable_effect(lease(), Effect::MyEffect);
}
