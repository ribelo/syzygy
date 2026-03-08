use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {}

fn lease() -> TaskLease {
    static LEASE: OnceLock<TaskLease> = OnceLock::new();
    LEASE.get_or_init(TaskLease::new).clone()
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Cancel => Command::cancel(lease()),
    }
}

#[test]
fn cancel_requires_assertion_in_exhaustive_mode() {
    assert_panic_contains("must assert", || {
        let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
        store.send(Event::Cancel);
    });
}
