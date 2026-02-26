use syzygy::command::Command;
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Cancel => Command::cancel(1),
    }
}

#[test]
fn assert_no_effects_requires_cancel_assertion() {
    let mut store = TestStore::new((), handler);
    store.send(Event::Cancel);

    assert_panic_contains("expected no emitted effects/cancellations", || {
        store.assert_no_effects();
    });

    store.assert_cancelled(1);
    store.assert_no_effects();
}
