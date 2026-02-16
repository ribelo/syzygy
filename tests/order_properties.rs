#![cfg(feature = "shell")]

use proptest::collection::vec;
use proptest::prelude::*;
use std::time::Duration;
use syzygy::command::Command;
use syzygy::executor::Task;
use syzygy::prelude::Syzygy;
use syzygy::syzygy::SyzygyConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropEvent {
    Primary(u16, bool),
    Secondary(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropEffect {
    EmitSecondary(u16),
}

#[derive(Default)]
struct PropModel {
    processed: Vec<PropEvent>,
}

fn update(event: PropEvent, model: &mut PropModel) -> Command<PropEvent, PropEffect> {
    match event {
        PropEvent::Primary(id, spawn_extra) => {
            model.processed.push(PropEvent::Primary(id, spawn_extra));
            if spawn_extra {
                Command::effect(PropEffect::EmitSecondary(id))
            } else {
                Command::none()
            }
        }
        PropEvent::Secondary(id) => {
            model.processed.push(PropEvent::Secondary(id));
            Command::none()
        }
    }
}

fn effects(effect: PropEffect, _resources: ()) -> Task<PropEvent, PropEffect> {
    match effect {
        PropEffect::EmitSecondary(id) => Task::event(PropEvent::Secondary(id)),
    }
}

proptest! {
    #[test]
    fn preserves_fifo_order(cases in vec((0u16..256u16, proptest::bool::ANY), 0..24)) {
        let mut runner = Syzygy::builder::<PropEvent, PropEffect>()
            .model(PropModel::default())
            .event_handler(update)
            .effect_handler(effects)
            .with_effect_channel_capacity(Some(256))
            .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
            .build();

        for (id, spawn_extra) in &cases {
            runner
                .core_mut()
                .try_send_event(PropEvent::Primary(*id, *spawn_extra))
                .expect("core event channel open");
        }

        let max_steps = (cases.len() * 2).max(1);
        runner.drain_max(max_steps).expect("drain should succeed");

        let processed = runner.core().model().processed.clone();

        let mut expected = Vec::with_capacity(cases.len() * 2);
        let mut secondary_tail = Vec::new();
        for (id, spawn_extra) in &cases {
            expected.push(PropEvent::Primary(*id, *spawn_extra));
            if *spawn_extra {
                secondary_tail.push(PropEvent::Secondary(*id));
            }
        }
        expected.extend(secondary_tail);

        prop_assert_eq!(processed, expected);
    }
}
