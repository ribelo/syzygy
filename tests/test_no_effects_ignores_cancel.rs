use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, TestStore};

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
fn assert_no_effects_requires_cancel_assertion() {
    let mut store = TestStore::new((), handler);
    store.send(Event::Cancel);

    assert_panic_contains("expected no emitted effects/cancellations", || {
        store.assert_no_effects();
    });

    store.assert_cancelled(lease());
    store.assert_no_effects();
}
