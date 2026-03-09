use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
use syzygy::extract::EventContext;
use syzygy::test_store::{Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    EmitTwo,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    A,
    B,
}

fn lease() -> TaskLease {
    static LEASE: OnceLock<TaskLease> = OnceLock::new();
    LEASE.get_or_init(TaskLease::new).clone()
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::EmitTwo => Command::abortable(lease(), Effect::A).and_abortable(lease(), Effect::B),
    }
}

#[test]
fn abortable_overwrite_records_implicit_cancellation_for_exhaustivity() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
    store.send(Event::EmitTwo);

    store.assert_cancelled(lease());
    store.assert_abortable_effect(lease(), Effect::B);
    store.assert_lease_empty(lease());
}
