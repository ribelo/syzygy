use syzygy::command::Command;
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

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::EmitTwo => Command::track(1, Effect::A).and_track(1, Effect::B),
    }
}

#[test]
fn overwrite_records_implicit_cancellation_for_exhaustivity() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
    store.send(Event::EmitTwo);

    store.assert_cancelled(1);
    store.assert_tracked_effect(1, Effect::B);
    store.assert_slot_empty(1);
}
