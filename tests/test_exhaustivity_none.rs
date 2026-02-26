use syzygy::command::Command;
use syzygy::extract::EventContext;
use syzygy::test_store::{assert_panic_contains, Exhaustivity, TestStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    EmitEffect,
    EmitNone,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Log,
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::EmitEffect => Command::effect(Effect::Log),
        Event::EmitNone => Command::none(),
    }
}

#[test]
fn none_command_does_not_mask_unasserted_effects() {
    assert_panic_contains("must assert effects", || {
        let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
        store.send(Event::EmitEffect);
        store.send(Event::EmitNone); // This should panic because previous effect was not asserted!
    });
}
