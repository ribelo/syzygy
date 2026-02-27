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

    fn handle_effect(
        &self,
        effect: Effect,
        _ctx: &EffectContext<'_>,
    ) -> Option<Task<Event, Effect>> {
        match effect {
            Effect::Trigger => Some(Task::send(Event::Done)),
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

#[test]
fn feature_builder_rejects_effect_handler_override() {
    assert_panic_contains("effect handler is already configured", || {
        let _ = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .feature(SampleFeature)
            .effect_handler(|_effect: Effect, _ctx: &EffectContext<'_>| Task::none());
    });
}

#[test]
fn feature_builder_chain_effect_handler_preserves_feature_handler() {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .feature(SampleFeature)
        .chain_effect_handler(|_effect: Effect, _ctx: &EffectContext<'_>| Task::none())
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done);
}

#[test]
fn chain_effect_handler_treats_task_none_as_handled() {
    #[derive(Debug, Clone)]
    enum LocalEvent {
        Start,
    }

    #[derive(Debug, Clone)]
    enum LocalEffect {
        Log,
    }

    let mut runner = Syzygy::builder::<LocalEvent, LocalEffect>()
        .model(())
        .event_handler(|event, _ctx| match event {
            LocalEvent::Start => Command::effect(LocalEffect::Log),
        })
        .effect_handler(|effect, _ctx| match effect {
            LocalEffect::Log => Task::none(),
        })
        .chain_effect_handler(|_effect, _ctx| -> Task<LocalEvent, LocalEffect> {
            panic!("fallback called")
        })
        .build();

    runner.core().try_send_event(LocalEvent::Start).unwrap();
    runner.step().unwrap();
}

#[test]
fn chain_effect_handler_supports_explicit_optional_fallthrough() {
    #[derive(Debug, Clone)]
    enum LocalEvent {
        Start,
        Completed,
    }

    #[derive(Debug, Clone)]
    enum LocalEffect {
        Work,
    }

    #[derive(Debug, Default, Model)]
    struct LocalModel {
        done_flag: bool,
    }

    let mut runner = Syzygy::builder::<LocalEvent, LocalEffect>()
        .model(LocalModel::default())
        .event_handler(|event, ctx| match event {
            LocalEvent::Start => Command::effect(LocalEffect::Work),
            LocalEvent::Completed => {
                let done_flag = DoneFlag::extract_mut(ctx);
                **done_flag = true;
                Command::none()
            }
        })
        .effect_handler(|_effect, _ctx| Option::<Task<LocalEvent, LocalEffect>>::None)
        .chain_effect_handler(|effect, _ctx| match effect {
            LocalEffect::Work => Task::send(LocalEvent::Completed),
        })
        .build();

    runner.core().try_send_event(LocalEvent::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done_flag);
}

#[test]
fn feature_builder_allows_fallback_handler_for_unhandled_effects() {
    struct FallbackFeature;

    impl Feature for FallbackFeature {
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

        fn handle_effect(
            &self,
            _effect: Effect,
            _ctx: &EffectContext<'_>,
        ) -> Option<Task<Event, Effect>> {
            None
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .feature(FallbackFeature)
        .chain_effect_handler(|effect, _ctx| match effect {
            Effect::Trigger => Task::send(Event::Done),
        })
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done);
}
