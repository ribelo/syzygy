use std::time::{Duration, Instant};

use syzygy::error::CoreError;
use syzygy::prelude::*;

#[test]
fn bounded_event_channel_surfaces_channel_full() {
    let (_core, tx) = Core::with_event_channel_capacity(
        Box::new(|_event: u32, _ctx: &EventContext<()>| Command::<u32, ()>::none()),
        (),
        Some(1),
    );

    let mut saw_full = false;
    for n in 0..10_000 {
        match tx.try_send_event(n) {
            Ok(()) => {}
            Err(CoreError::ChannelFull) => {
                saw_full = true;
                break;
            }
            Err(other) => panic!("unexpected sender error: {other:?}"),
        }
    }

    assert!(
        saw_full,
        "expected bounded event channel to report ChannelFull"
    );
}

#[test]
fn run_until_progresses_async_effects_inside_runtime() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Wait,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        done: bool,
    }

    fn dispatch(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Wait),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn effects(effect: Effect, _ctx: &EffectContext) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async {
                compio::runtime::time::sleep(Duration::from_millis(1)).await;
                Command::event(Event::Done)
            }),
        }
    }

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .event_handler(dispatch)
            .effect_handler(effects)
            .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        let started = Instant::now();
        runner
            .run_until(|core, _shell| core.model().done)
            .expect("run_until should complete");

        assert!(runner.model().done);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "run_until took too long: {:?}",
            started.elapsed()
        );
    });
}

#[test]
fn cancelling_tracked_future_returns_shell_to_idle() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stop,
        StartAndStop,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Wait,
    }

    fn dispatch(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::track(1_u8, Effect::Wait),
            Event::Stop => Command::cancel(1_u8),
            Event::StartAndStop => Command::track(1_u8, Effect::Wait).and_cancel(1_u8),
        }
    }

    fn effects(effect: Effect, _ctx: &EffectContext) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async {
                compio::runtime::time::sleep(Duration::from_secs(60)).await;
                Command::none()
            }),
        }
    }

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(())
            .event_handler(dispatch)
            .effect_handler(effects)
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();
        assert!(!runner.shell().is_idle());

        runner.core().try_send_event(Event::Stop).unwrap();
        runner.step().unwrap();
        compio::runtime::time::sleep(Duration::from_millis(2)).await;

        assert!(
            runner.shell().is_idle(),
            "shell should be idle after cancellation"
        );

        let mut same_step_runner = Syzygy::builder::<Event, Effect>()
            .model(())
            .event_handler(dispatch)
            .effect_handler(effects)
            .build();
        same_step_runner
            .core()
            .try_send_event(Event::StartAndStop)
            .unwrap();
        same_step_runner.step().unwrap();
        compio::runtime::time::sleep(Duration::from_millis(2)).await;
        assert!(
            same_step_runner.shell().is_idle(),
            "shell should be idle when tracked task is cancelled in the same command"
        );
    });
}

#[test]
fn deep_resolved_effect_chain_is_iterative() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Loop(u32),
    }

    const DEPTH: u32 = 20_000;

    fn dispatch(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Loop(DEPTH)),
        }
    }

    fn effects(effect: Effect, _ctx: &EffectContext) -> Task<Event, Effect> {
        match effect {
            Effect::Loop(0) => Task::none(),
            Effect::Loop(n) => Task::resolved(Command::effect(Effect::Loop(n - 1))),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(dispatch)
        .effect_handler(effects)
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner.step().unwrap();

    assert!(runner.shell().is_idle());
}

#[test]
fn spawned_events_are_deferred_when_event_channel_is_full() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Block,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        AsyncDone,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        done: bool,
    }

    fn dispatch(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::event(Event::Block).and_effect(Effect::AsyncDone),
            Event::Block => Command::none(),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn effects(effect: Effect, _ctx: &EffectContext) -> Task<Event, Effect> {
        match effect {
            Effect::AsyncDone => Task::once(async { Command::event(Event::Done) }),
        }
    }

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .event_handler(dispatch)
            .effect_handler(effects)
            .with_event_channel_capacity(Some(1))
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();
        assert!(!runner.model().done);

        runner.step().unwrap();
        assert!(!runner.model().done);

        runner.step().unwrap();
        assert!(runner.model().done);
    });
}

#[test]
fn future_effects_can_resolve_without_compio_runtime() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        done: bool,
    }

    fn dispatch(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Work),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn effects(effect: Effect, _ctx: &EffectContext) -> Task<Event, Effect> {
        match effect {
            Effect::Work => Task::once(async { Command::event(Event::Done) }),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(dispatch)
        .effect_handler(effects)
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done);
}
