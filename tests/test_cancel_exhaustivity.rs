use syzygy::command::Command;
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, Exhaustivity, TestStore};

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
fn cancel_requires_assertion_in_exhaustive_mode() {
    assert_panic_contains("must assert", || {
        let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
        store.send(Event::Cancel);
    });
}
