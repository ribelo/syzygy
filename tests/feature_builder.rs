use syzygy::feature::Feature;
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum Event {
    Start,
    Done,
}

#[derive(Debug, Clone)]
enum Effect {
    Trigger,
}

#[derive(Debug, Default, Model)]
struct Model {
    done: bool,
}

struct SampleFeature;

impl Feature for SampleFeature {
    type State = Model;
    type Event = Event;
    type Effect = Effect;

    fn reduce(&self, event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Trigger),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn handle_effect(&self, effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Trigger => Task::send(Event::Done),
        }
    }
}

#[test]
fn feature_builder_wires_reduce_and_handle_effect() {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .feature(SampleFeature)
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done);
}
