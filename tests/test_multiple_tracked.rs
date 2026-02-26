use syzygy::command::Command;
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Step1,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    MyEffect1,
    MyEffect2,
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Step1 => Command::track(1, Effect::MyEffect1).and_track(2, Effect::MyEffect2),
    }
}

#[test]
fn drop_panics_when_other_tracked_effects_remain_unasserted() {
    assert_panic_contains("must assert effects before dropping test store", || {
        let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
        store.send(Event::Step1);
        store.assert_tracked_effect(1, Effect::MyEffect1);
    });
}
