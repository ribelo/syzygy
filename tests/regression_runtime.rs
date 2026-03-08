use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{stream, StreamExt};
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
        match tx.try_send(n) {
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
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
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
        .build()
        .unwrap();

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
        .build()
        .unwrap();

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
fn run_until_progresses_async_effects() {
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
                syzygy::runtime::sleep(Duration::from_millis(1)).await;
                Command::event(Event::Done)
            }),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
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

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .with_resource(CloneCounter(Arc::clone(&clone_count)))
        .event_handler(|event, _ctx| match event {
            Event::Start => Command::batch((0..128).map(|_| Command::effect(Effect::Work))),
        })
        .effect_handler(|effect, _ctx| match effect {
            Effect::Work => Task::once(async { Command::none() }),
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.run().unwrap();

    assert_eq!(clone_count.load(Ordering::SeqCst), 0);
}

#[test]
fn cancelling_abortable_future_returns_shell_to_idle() {
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

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async {
                syzygy::runtime::sleep(Duration::from_secs(60)).await;
                Command::none()
            }),
        }
    }

    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler({
            let lease = lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Wait),
                Event::Stop => Command::cancel(&lease),
                Event::StartAndStop => Command::abortable(&lease, Effect::Wait).and_cancel(&lease),
            }
        })
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    assert!(!runner.shell().is_idle());

    runner.core().try_send(Event::Stop).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(
        runner.shell().is_idle(),
        "shell should be idle after cancellation"
    );

    let same_step_lease = TaskLease::new();
    let mut same_step_runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler({
            let same_step_lease = same_step_lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&same_step_lease, Effect::Wait),
                Event::Stop => Command::cancel(&same_step_lease),
                Event::StartAndStop => {
                    Command::abortable(&same_step_lease, Effect::Wait).and_cancel(&same_step_lease)
                }
            }
        })
        .effect_handler(handle_effect)
        .build()
        .unwrap();
    same_step_runner
        .core()
        .try_send(Event::StartAndStop)
        .unwrap();
    same_step_runner.step().unwrap();
    same_step_runner
        .run_until(|_, shell| shell.is_idle())
        .unwrap();
    assert!(
        same_step_runner.shell().is_idle(),
        "shell should be idle when abortable task is cancelled in the same command"
    );
}

#[test]
fn cancelled_abortable_future_does_not_route_completion_event() {
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

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async {
                syzygy::runtime::sleep(Duration::from_millis(30)).await;
                Command::event(Event::Done)
            }),
        }
    }

    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler({
            let lease = lease.clone();
            move |event, ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Wait),
                Event::Stop => Command::cancel(&lease),
                Event::Done => {
                    let done = Done::extract_mut(ctx);
                    **done = true;
                    Command::none()
                }
            }
        })
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    runner.core().try_send(Event::Stop).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(!runner.model().done);
}

#[test]
fn dropping_abortable_lease_cancels_running_future() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        DropLease,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Wait,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        lease: Option<TaskLease>,
        done: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => {
                let lease = TaskLease::new();
                let current = Lease::extract_mut(ctx);
                **current = Some(lease.clone());
                Command::abortable(lease, Effect::Wait)
            }
            Event::DropLease => {
                let current = Lease::extract_mut(ctx);
                **current = None;
                Command::none()
            }
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
                syzygy::runtime::sleep(Duration::from_millis(30)).await;
                Command::event(Event::Done)
            }),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    assert!(!runner.shell().is_idle());

    runner.core().try_send(Event::DropLease).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(
        runner.shell().is_idle(),
        "shell should be idle after abortable lease ownership disappears"
    );
    assert!(!runner.model().done);
}

#[test]
fn dropping_abortable_lease_cancels_running_stream() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        DropLease,
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Watch,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        lease: Option<TaskLease>,
        ticks: usize,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => {
                let lease = TaskLease::new();
                let current = Lease::extract_mut(ctx);
                **current = Some(lease.clone());
                Command::abortable(lease, Effect::Watch)
            }
            Event::DropLease => {
                let current = Lease::extract_mut(ctx);
                **current = None;
                Command::none()
            }
            Event::Tick => {
                let ticks = Ticks::extract_mut(ctx);
                **ticks += 1;
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Watch => Task::stream(
                stream::once(async {
                    syzygy::runtime::sleep(Duration::from_millis(30)).await;
                    Command::event(Event::Tick)
                })
                .chain(stream::once(async {
                    syzygy::runtime::sleep(Duration::from_millis(30)).await;
                    Command::event(Event::Tick)
                })),
            ),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    assert!(!runner.shell().is_idle());

    runner.core().try_send(Event::DropLease).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(
        runner.shell().is_idle(),
        "shell should be idle after abortable stream ownership disappears"
    );
    assert_eq!(runner.model().ticks, 0);
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
                syzygy::runtime::sleep(Duration::from_millis(2)).await;
                Command::event(Event::Tick)
            }),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.shutdown();
    runner.step().unwrap();
    runner.step().unwrap();

    assert_eq!(
        runner.model().ticks,
        0,
        "spawned commands should be ignored after shutdown"
    );
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

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(|event, _ctx| match event {
            Event::Start => Command::effect(Effect::Flush),
        })
        .effect_handler(move |effect, _ctx| {
            let finished_in_effect = Arc::clone(&finished_in_effect);
            match effect {
                Effect::Flush => Task::once(async move {
                    syzygy::runtime::sleep(Duration::from_secs(60)).await;
                    finished_in_effect.store(true, Ordering::SeqCst);
                    Command::none()
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
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
}

#[test]
fn shutdown_cancels_abortable_async_tasks_without_refcell_reentrancy_panics() {
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

    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler({
            let lease = lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Flush),
            }
        })
        .effect_handler(move |effect, _ctx| {
            let finished_in_effect = Arc::clone(&finished_in_effect);
            match effect {
                Effect::Flush => Task::once(async move {
                    syzygy::runtime::sleep(Duration::from_secs(60)).await;
                    finished_in_effect.store(true, Ordering::SeqCst);
                    Command::none()
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    let shutdown_started = Instant::now();
    runner.shutdown();
    let elapsed = shutdown_started.elapsed();

    assert!(
        elapsed < Duration::from_secs(1),
        "shutdown should cancel abortable tasks promptly: {elapsed:?}"
    );
    assert!(
        !finished.load(Ordering::SeqCst),
        "abortable task should be cancelled before completion"
    );
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
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
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

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .with_event_channel_capacity(Some(1))
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    assert!(!runner.model().done);

    runner.step().unwrap();
    assert!(!runner.model().done);

    runner.step().unwrap();
    assert!(runner.model().done);
}

#[test]
fn runtime_yield_now_reschedules_the_current_task() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let order_for_runtime = Rc::clone(&order);

    let runtime = syzygy::runtime::Runtime::new().unwrap();
    runtime.block_on({
        let runtime = runtime.clone();
        async move {
            let order_for_task = Rc::clone(&order_for_runtime);
            let handle = runtime.spawn(async move {
                order_for_task.borrow_mut().push("other");
            });

            order_for_runtime.borrow_mut().push("before");
            syzygy::runtime::yield_now().await;
            order_for_runtime.borrow_mut().push("after");

            match handle.await {
                Ok(()) => {}
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
    });

    assert_eq!(order.borrow().as_slice(), ["before", "other", "after"]);
}

#[test]
fn future_effects_resolve_on_syzygys_owned_runtime() {
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
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert!(runner.model().done);
}
