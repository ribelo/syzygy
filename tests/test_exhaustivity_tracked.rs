use syzygy::command::Command;
use syzygy::extract::EventContext;
use syzygy::test_store::{Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Step1,
    Step2,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    MyEffect,
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Step1 => Command::track(1, Effect::MyEffect),
        Event::Step2 => Command::none(),
    }
}

#[test]
fn tracked_assertion_drains_effect_for_exhaustivity() {
    let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
    store.send(Event::Step1);
    store.assert_tracked_effect(1, Effect::MyEffect);
    store.send(Event::Step2);
}
