use std::sync::OnceLock;

use syzygy::command::{Command, TaskLease};
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

fn first_lease() -> TaskLease {
    static LEASE: OnceLock<TaskLease> = OnceLock::new();
    LEASE.get_or_init(TaskLease::new).clone()
}

fn second_lease() -> TaskLease {
    static LEASE: OnceLock<TaskLease> = OnceLock::new();
    LEASE.get_or_init(TaskLease::new).clone()
}

fn handler(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Step1 => Command::abortable(first_lease(), Effect::MyEffect1)
            .and_abortable(second_lease(), Effect::MyEffect2),
    }
}

#[test]
fn drop_panics_when_other_abortable_effects_remain_unasserted() {
    assert_panic_contains("must assert effects before dropping test store", || {
        let mut store = TestStore::new((), handler).with_exhaustivity(Exhaustivity::On);
        store.send(Event::Step1);
        store.assert_abortable_effect(first_lease(), Effect::MyEffect1);
    });
}
