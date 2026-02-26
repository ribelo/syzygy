use syzygy::command::Command;
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    CancelIt,
    DoNothing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::CancelIt => Command::cancel(1),
        Event::DoNothing => Command::none(),
    }
}

#[test]
fn cancelled_assertion_consumes_slot() {
    let mut store = TestStore::new((), handler);
    store.send(Event::CancelIt);
    store.assert_cancelled(1);

    store.send(Event::DoNothing);
    assert_panic_contains("to be cancelled", || {
        store.assert_cancelled(1);
    });
}
