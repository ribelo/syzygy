use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    CancelIt,
    DoNothing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {}

fn lease() -> TaskLease {
    static LEASE: OnceLock<TaskLease> = OnceLock::new();
    LEASE.get_or_init(TaskLease::new).clone()
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::CancelIt => Command::cancel(lease()),
        Event::DoNothing => Command::none(),
    }
}

#[test]
fn cancelled_assertion_consumes_slot() {
    let mut store = TestStore::new((), handler);
    store.send(Event::CancelIt);
    store.assert_cancelled(lease());

    store.send(Event::DoNothing);
    assert_panic_contains("to be cancelled", || {
        store.assert_cancelled(lease());
    });
}
