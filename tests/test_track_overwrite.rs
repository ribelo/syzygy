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
fn overwrite_drops_effect_without_exhaustivity_error() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
    store.send(Event::EmitTwo);
    // User only asserts B. Is this allowed? Yes, currently.
    store.assert_tracked_effect(1, Effect::B);
}
