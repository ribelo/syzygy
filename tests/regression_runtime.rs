use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
fn bounded_channel_overflow_does_not_crash() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        A,
        B,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Default, Model)]
    struct Model {
        seen: usize,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::event(Event::A).and_event(Event::B),
            Event::A | Event::B => {
                let seen = Seen::extract_mut(ctx);
                **seen += 1;
                Command::none()
            }
        }
    }

    fn handle_effect(_effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        Task::none()
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .with_event_channel_capacity(Some(1))
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner
        .step()
        .expect("bounded overflow should defer events instead of failing");
    runner.step().unwrap();
    runner.step().unwrap();

    assert_eq!(runner.model().seen, 2);
}

#[test]
fn deferred_events_preserve_fifo_when_new_commands_arrive() {
    #[derive(Debug, Clone)]
    enum Event {
        A,
        B,
        C,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Default, Model)]
    struct Model {
        order: Vec<&'static str>,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::A => Command::none(),
            Event::B => {
                let order = Order::extract_mut(ctx);
                order.push("b");
                Command::none()
            }
            Event::C => {
                let order = Order::extract_mut(ctx);
                order.push("c");
                Command::none()
            }
        }
    }

    fn handle_effect(_effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        Task::none()
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .with_event_channel_capacity(Some(1))
        .build();

    runner
        .shell_mut()
        .dispatch_command(Command::event(Event::A).and_event(Event::B))
        .unwrap();

    let first_commands = runner.core_mut().process_events();
    for command in first_commands {
        runner.shell_mut().dispatch_command(command).unwrap();
    }

    runner
        .shell_mut()
        .dispatch_command(Command::event(Event::C))
        .unwrap();

    for _ in 0..3 {
        runner.step().unwrap();
    }

    assert_eq!(runner.model().order, ["b", "c"]);
}

#[test]
fn run_until_exits_when_shell_is_closed() {
    #[derive(Debug, Clone)]
    enum Event {
        Ping,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    fn handle_event(_event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        Command::none()
    }

    fn handle_effect(_effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        Task::none()
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build();

    let _ = Event::Ping;
    runner.shutdown();
    let started = Instant::now();
    runner
        .run_until(|_, _| started.elapsed() > Duration::from_millis(50))
        .unwrap();

    assert!(
        started.elapsed() < Duration::from_millis(20),
        "run_until should return quickly once shell is closed"
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

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Wait),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
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
            .event_handler(handle_event)
            .effect_handler(handle_effect)
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
fn spawning_async_effects_does_not_clone_registered_resources() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug)]
    struct CloneCounter(Arc<std::sync::atomic::AtomicUsize>);

    impl Clone for CloneCounter {
        fn clone(&self) -> Self {
            self.0.fetch_add(1, Ordering::SeqCst);
            Self(Arc::clone(&self.0))
        }
    }

    let clone_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(())
            .with_resource(CloneCounter(Arc::clone(&clone_count)))
            .event_handler(|event, _ctx| match event {
                Event::Start => Command::batch((0..128).map(|_| Command::effect(Effect::Work))),
            })
            .effect_handler(|effect, _ctx| match effect {
                Effect::Work => Task::once(async { Command::none() }),
            })
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();
        runner.run().unwrap();
    });

    assert_eq!(clone_count.load(Ordering::SeqCst), 0);
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

    fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::track(1_u8, Effect::Wait),
            Event::Stop => Command::cancel(1_u8),
            Event::StartAndStop => Command::track(1_u8, Effect::Wait).and_cancel(1_u8),
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
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
            .event_handler(handle_event)
            .effect_handler(handle_effect)
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
            .event_handler(handle_event)
            .effect_handler(handle_effect)
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
fn cancelled_tracked_future_does_not_route_completion_event() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stop,
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

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::track(1_u8, Effect::Wait),
            Event::Stop => Command::cancel(1_u8),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async {
                compio::runtime::time::sleep(Duration::from_millis(30)).await;
                Command::event(Event::Done)
            }),
        }
    }

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .event_handler(handle_event)
            .effect_handler(handle_effect)
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();

        runner.core().try_send_event(Event::Stop).unwrap();
        runner.step().unwrap();

        compio::runtime::time::sleep(Duration::from_millis(80)).await;
        runner.step().unwrap();
        runner.step().unwrap();

        assert!(!runner.model().done);
    });
}

#[test]
fn shutdown_blocks_spawned_untracked_command_routing() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Loop,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        ticks: usize,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Loop),
            Event::Tick => {
                let ticks = Ticks::extract_mut(ctx);
                **ticks += 1;
                Command::effect(Effect::Loop)
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Loop => Task::once(async {
                compio::runtime::time::sleep(Duration::from_millis(2)).await;
                Command::event(Event::Tick)
            }),
        }
    }

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .event_handler(handle_event)
            .effect_handler(handle_effect)
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();
        runner.shutdown();

        compio::runtime::time::sleep(Duration::from_millis(8)).await;
        runner.step().unwrap();
        runner.step().unwrap();

        assert_eq!(
            runner.model().ticks,
            0,
            "spawned commands should be ignored after shutdown"
        );
    });
}

#[test]
fn shutdown_cancels_untracked_async_tasks_without_timeout() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Flush,
    }

    let finished = Arc::new(AtomicBool::new(false));
    let finished_in_effect = Arc::clone(&finished);

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async move {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(())
            .event_handler(|event, _ctx| match event {
                Event::Start => Command::effect(Effect::Flush),
            })
            .effect_handler(move |effect, _ctx| {
                let finished_in_effect = Arc::clone(&finished_in_effect);
                match effect {
                    Effect::Flush => Task::once(async move {
                        compio::runtime::time::sleep(Duration::from_secs(60)).await;
                        finished_in_effect.store(true, Ordering::SeqCst);
                        Command::none()
                    }),
                }
            })
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();

        let shutdown_started = Instant::now();
        runner.shutdown();
        let elapsed = shutdown_started.elapsed();

        assert!(
            elapsed < Duration::from_secs(1),
            "shutdown should cancel untracked tasks promptly: {elapsed:?}"
        );
        assert!(
            !finished.load(Ordering::SeqCst),
            "untracked task should be cancelled before completion"
        );
    });
}

#[test]
fn shutdown_cancels_tracked_async_tasks_without_refcell_reentrancy_panics() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Flush,
    }

    let finished = Arc::new(AtomicBool::new(false));
    let finished_in_effect = Arc::clone(&finished);

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async move {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(())
            .event_handler(|event, _ctx| match event {
                Event::Start => Command::track("slot", Effect::Flush),
            })
            .effect_handler(move |effect, _ctx| {
                let finished_in_effect = Arc::clone(&finished_in_effect);
                match effect {
                    Effect::Flush => Task::once(async move {
                        compio::runtime::time::sleep(Duration::from_secs(60)).await;
                        finished_in_effect.store(true, Ordering::SeqCst);
                        Command::none()
                    }),
                }
            })
            .build();

        runner.core().try_send_event(Event::Start).unwrap();
        runner.step().unwrap();

        let shutdown_started = Instant::now();
        runner.shutdown();
        let elapsed = shutdown_started.elapsed();

        assert!(
            elapsed < Duration::from_secs(1),
            "shutdown should cancel tracked tasks promptly: {elapsed:?}"
        );
        assert!(
            !finished.load(Ordering::SeqCst),
            "tracked task should be cancelled before completion"
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

    fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Loop(DEPTH)),
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Loop(0) => Task::none(),
            Effect::Loop(n) => Task::resolved(Command::effect(Effect::Loop(n - 1))),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
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

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
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

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::AsyncDone => Task::once(async { Command::event(Event::Done) }),
        }
    }

    let rt = compio::runtime::Runtime::new().expect("compio runtime");
    rt.block_on(async {
        let mut runner = Syzygy::builder::<Event, Effect>()
            .model(Model::default())
            .event_handler(handle_event)
            .effect_handler(handle_effect)
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

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Work),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Work => Task::once(async { Command::event(Event::Done) }),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build();

    runner.core().try_send_event(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done);
}
