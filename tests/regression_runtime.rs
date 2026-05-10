use std::cell::{Cell, RefCell};
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{stream, StreamExt};
use syzygy::error::{CoreError, ShellError};
use syzygy::prelude::*;
use syzygy::shell::{
    ShellCancellationReason, ShellTerminationReason, ShellTerminationTarget, ShellTraceCommandStep,
    ShellTraceConfig, ShellTraceEvent, ShellTraceTaskKind,
};

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
        #[model(wrapper = Seen)]
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
fn unhandled_effect_without_handler_fails_fast() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        Command::effect(Effect::Work)
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();

    let err = runner.step().unwrap_err();
    assert_eq!(
        err,
        ShellError::UnhandledEffect {
            effect_type: std::any::type_name::<Effect>(),
        }
    );
}

#[test]
fn shell_dispatch_error_does_not_rollback_committed_model_mutation() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Counter)]
        counter: usize,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => {
                let counter = Counter::extract_mut(ctx);
                **counter += 1;
                Command::effect(Effect::Work)
            }
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();

    let error = runner.step().expect_err("unhandled effect must fail fast");
    assert!(matches!(error, ShellError::UnhandledEffect { .. }));
    assert_eq!(runner.model().counter, 1);
}

#[test]
fn chained_effect_handlers_fallthrough_fail_fast() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        Command::effect(Effect::Work)
    }

    fn first(_effect: Effect, _ctx: &EffectContext<'_>) -> Option<Task<Event, Effect>> {
        None
    }

    fn second(_effect: Effect, _ctx: &EffectContext<'_>) -> Option<Task<Event, Effect>> {
        None
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(first)
        .chain_effect_handler(second)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();

    let err = runner.step().unwrap_err();
    assert_eq!(
        err,
        ShellError::UnhandledEffect {
            effect_type: std::any::type_name::<Effect>(),
        }
    );
}

#[test]
fn permissive_unhandled_effect_policy_preserves_legacy_drop_behavior() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Seen)]
        seen: bool,
    }

    fn handle_event(_event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        let seen = Seen::extract_mut(ctx);
        **seen = true;
        Command::effect(Effect::Work)
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .with_syzygy_config(
            SyzygyConfig::default().unhandled_effects(UnhandledEffectPolicy::Ignore),
        )
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();

    assert!(runner.step().unwrap());
    assert!(runner.model().seen);
    assert!(runner.shell().is_idle());
}

#[test]
fn set_config_updates_unhandled_effect_policy_live() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        Command::effect(Effect::Work)
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .with_syzygy_config(
            SyzygyConfig::default().unhandled_effects(UnhandledEffectPolicy::Ignore),
        )
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    assert!(runner.step().unwrap());

    runner.set_config(SyzygyConfig::default());
    runner.core().try_send(Event::Start).unwrap();

    let err = runner.step().unwrap_err();
    assert_eq!(
        err,
        ShellError::UnhandledEffect {
            effect_type: std::any::type_name::<Effect>(),
        }
    );
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
        #[model(wrapper = Order)]
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
fn command_events_are_deferred_to_next_step_and_run_before_later_external_ingress() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        A,
        B,
        External,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Order)]
        order: Vec<&'static str>,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::event(Event::A).and_event(Event::B),
            Event::A => {
                let order = Order::extract_mut(ctx);
                order.push("a");
                Command::none()
            }
            Event::B => {
                let order = Order::extract_mut(ctx);
                order.push("b");
                Command::none()
            }
            Event::External => {
                let order = Order::extract_mut(ctx);
                order.push("x");
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
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    assert!(runner.model().order.is_empty());

    runner.core().try_send(Event::External).unwrap();
    runner.step().unwrap();
    assert_eq!(runner.model().order, ["a", "b", "x"]);
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
fn run_until_timeout_returns_timeout_error() {
    fn handle_event(_event: (), _ctx: &EventContext<()>) -> Command<(), ()> {
        Command::none()
    }

    let mut runner = Syzygy::builder::<(), ()>()
        .model(())
        .event_handler(handle_event)
        .build()
        .unwrap();

    let timeout = Duration::from_millis(10);
    let error = runner
        .run_until_timeout(timeout, |_, _| false)
        .expect_err("expected timeout for unsatisfied condition");
    assert_eq!(error, ShellError::Timeout { duration: timeout });
}

#[test]
fn run_until_deadline_returns_timeout_error() {
    fn handle_event(_event: (), _ctx: &EventContext<()>) -> Command<(), ()> {
        Command::none()
    }

    let mut runner = Syzygy::builder::<(), ()>()
        .model(())
        .event_handler(handle_event)
        .build()
        .unwrap();

    let timeout = Duration::from_millis(10);
    let deadline = Instant::now().checked_add(timeout).unwrap();
    let error = runner
        .run_until_deadline(deadline, |_, _| false)
        .expect_err("expected timeout for unsatisfied condition");
    match error {
        ShellError::Timeout { duration } => {
            assert!(
                duration >= timeout,
                "timeout duration should be at least requested timeout"
            );
        }
        other => panic!("expected timeout error, got {other:?}"),
    }
}

#[test]
fn run_until_or_cancelled_reports_exit_reason() {
    fn handle_event(_event: (), _ctx: &EventContext<()>) -> Command<(), ()> {
        Command::none()
    }

    let mut runner = Syzygy::builder::<(), ()>()
        .model(())
        .event_handler(handle_event)
        .build()
        .unwrap();

    let condition_met = runner
        .run_until_or_cancelled(|_, _| true, |_, _| false)
        .unwrap();
    assert_eq!(condition_met, RunUntilExit::ConditionMet);

    let cancelled = runner
        .run_until_or_cancelled(|_, _| false, |_, _| true)
        .unwrap();
    assert_eq!(cancelled, RunUntilExit::Cancelled);

    runner.shutdown();
    let shell_closed = runner
        .run_until_or_cancelled(|_, _| false, |_, _| false)
        .unwrap();
    assert_eq!(shell_closed, RunUntilExit::ShellClosed);
}

#[test]
fn boot_handler_runs_once_before_prequeued_events() {
    #[derive(Debug, Clone)]
    enum Event {
        Boot,
        External,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Order)]
        order: Vec<&'static str>,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        let order = Order::extract_mut(ctx);
        match event {
            Event::Boot => order.push("boot"),
            Event::External => order.push("external"),
        }
        Command::none()
    }

    fn boot(_model: &Model) -> Command<Event, Effect> {
        Command::event(Event::Boot)
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .boot_handler(boot)
        .build()
        .unwrap();

    runner.core().try_send(Event::External).unwrap();
    runner.step().unwrap();

    assert_eq!(runner.model().order, ["boot", "external"]);

    runner.core().try_send(Event::External).unwrap();
    runner.step().unwrap();

    assert_eq!(runner.model().order, ["boot", "external", "external"]);
}

#[test]
fn mixed_boot_command_processes_boot_event_before_dispatching_boot_effects() {
    #[derive(Debug, Clone)]
    enum Event {
        Boot,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        VerifyBootApplied,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Booted)]
        booted: bool,
    }

    let boot_applied = Rc::new(Cell::new(false));
    let boot_applied_in_event = Rc::clone(&boot_applied);
    let boot_applied_in_effect = Rc::clone(&boot_applied);

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(move |event: Event, ctx: &EventContext<Model>| {
            match event {
                Event::Boot => {
                    let booted = Booted::extract_mut(ctx);
                    **booted = true;
                    boot_applied_in_event.set(true);
                }
            }
            Command::none()
        })
        .effect_handler(move |effect: Effect, _ctx: &EffectContext<'_>| {
            match effect {
                Effect::VerifyBootApplied => {
                    assert!(
                        boot_applied_in_effect.get(),
                        "boot effect dispatched before boot event updated model state"
                    );
                }
            }
            Task::none()
        })
        .boot_handler(|_model: &Model| {
            Command::event(Event::Boot).and_effect(Effect::VerifyBootApplied)
        })
        .build()
        .unwrap();

    runner.step().unwrap();
    assert!(runner.model().booted);
}

#[test]
fn boot_is_preserved_across_split_and_from() {
    #[derive(Debug, Clone)]
    enum Event {
        Boot,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Count)]
        count: usize,
    }

    fn handle_event(_event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        let count = Count::extract_mut(ctx);
        **count += 1;
        Command::none()
    }

    fn boot(_model: &Model) -> Command<Event, Effect> {
        Command::event(Event::Boot)
    }

    let app = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .boot_handler(boot)
        .build()
        .unwrap();

    let (core, shell) = app.split();
    let mut runner = Syzygy::from((core, shell));
    runner.step().unwrap();

    assert_eq!(runner.model().count, 1);
}

#[test]
fn run_until_executes_pending_boot_even_when_condition_starts_true() {
    #[derive(Debug, Clone)]
    enum Event {
        Boot,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Booted)]
        booted: bool,
    }

    fn handle_event(_event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        let booted = Booted::extract_mut(ctx);
        **booted = true;
        Command::none()
    }

    fn boot(_model: &Model) -> Command<Event, Effect> {
        Command::event(Event::Boot)
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .boot_handler(boot)
        .build()
        .unwrap();

    runner.run_until(|_, _| true).unwrap();

    assert!(runner.model().booted);
}

#[test]
fn boot_runs_before_subscription_reconciliation() {
    #[derive(Debug, Clone)]
    enum Event {
        Enable,
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Clock,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
        #[model(wrapper = Ticks)]
        ticks: usize,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Enable => {
                let running = Running::extract_mut(ctx);
                **running = true;
                Command::none()
            }
            Event::Tick => {
                let ticks = Ticks::extract_mut(ctx);
                **ticks += 1;
                Command::none()
            }
        }
    }

    fn boot(_model: &Model) -> Command<Event, Effect> {
        Command::event(Event::Enable)
    }

    fn subscriptions(running: &Running) -> Subscription<Event, Effect> {
        if !**running {
            return Subscription::none();
        }

        Subscription::every(Key::Clock, Duration::from_millis(1), Event::Tick)
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
        handle!(subscriptions, ctx)
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .boot_handler(boot)
        .subscription_handler(handle_subscriptions)
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
        .build()
        .unwrap();

    runner.step().unwrap();

    assert!(runner.model().running);
    runner.run_until(|core, _| core.model().ticks > 0).unwrap();
    assert!(runner.model().ticks > 0);
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
        #[model(wrapper = Done)]
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
fn manual_clock_advances_async_effects_without_wall_time() {
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
        #[model(wrapper = Done)]
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
                syzygy::runtime::sleep(Duration::from_secs(60)).await;
                Command::event(Event::Done)
            }),
        }
    }

    let (runtime, clock) = syzygy::runtime::Runtime::manual().unwrap();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .with_runtime(runtime)
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    assert!(!runner.model().done);

    clock.advance(Duration::from_secs(59));
    runner.step().unwrap();
    runner.step().unwrap();
    assert!(!runner.model().done);

    clock.advance(Duration::from_secs(1));
    runner.step().unwrap();
    runner.step().unwrap();
    assert!(runner.model().done);
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
fn effect_context_resource_ref_borrows_without_cloning() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug)]
    struct CountedResource {
        clones: Arc<std::sync::atomic::AtomicUsize>,
        hits: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl Clone for CountedResource {
        fn clone(&self) -> Self {
            self.clones.fetch_add(1, Ordering::SeqCst);
            Self {
                clones: Arc::clone(&self.clones),
                hits: Arc::clone(&self.hits),
            }
        }
    }

    let clone_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hit_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .with_resource(CountedResource {
            clones: Arc::clone(&clone_count),
            hits: Arc::clone(&hit_count),
        })
        .event_handler(|event, _ctx| match event {
            Event::Start => Command::batch((0..64).map(|_| Command::effect(Effect::Work))),
        })
        .effect_handler(|effect, ctx| match effect {
            Effect::Work => {
                let resource = ctx
                    .resource_ref::<CountedResource>()
                    .expect("resource should be present");
                resource.hits.fetch_add(1, Ordering::SeqCst);
                Task::none()
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.run().unwrap();

    assert_eq!(hit_count.load(Ordering::SeqCst), 64);
    assert_eq!(clone_count.load(Ordering::SeqCst), 0);
}

#[test]
fn typed_effect_handler_injects_registered_resource() {
    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Status)]
        status: String,
    }

    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Saved(String),
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Save,
    }

    #[derive(Clone)]
    struct Prefix(&'static str);

    fn save(_: (), prefix: Prefix) -> Task<Event, Effect> {
        Task::send(Event::Saved(format!("{} saved", prefix.0)))
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Save),
            Event::Saved(status) => {
                **Status::extract_mut(ctx) = status;
                Command::none()
            }
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .with_resource(Prefix("typed"))
        .event_handler(handle_event)
        .typed_effect_handler(|effect, ctx| match effect {
            Effect::Save => handle!(save, ctx),
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.step().unwrap();

    assert_eq!(runner.model().status, "typed saved");
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
        #[model(wrapper = Done)]
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
        #[model(wrapper = Lease)]
        lease: AbortSlot,
        #[model(wrapper = Done)]
        done: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Lease::extract_mut(ctx).start(Effect::Wait),
            Event::DropLease => {
                let current = Lease::extract_mut(ctx);
                current.clear();
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
        #[model(wrapper = Lease)]
        lease: AbortSlot,
        #[model(wrapper = Ticks)]
        ticks: usize,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Lease::extract_mut(ctx).start(Effect::Watch),
            Event::DropLease => {
                let current = Lease::extract_mut(ctx);
                current.clear();
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
fn shell_snapshot_reports_active_task_subscription_and_queue_counts() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Wait,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Key {
        Clock,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
    }

    fn describe(running: &Running) -> Subscription<Event, Effect> {
        if !**running {
            return Subscription::none();
        }

        Subscription::every(Key::Clock, Duration::from_secs(60), Event::Tick)
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
        handle!(describe, ctx)
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async {
                syzygy::runtime::sleep(Duration::from_secs(60)).await;
                Command::none()
            }),
        }
    }

    let (runtime, _clock) = syzygy::runtime::Runtime::manual().unwrap();
    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model { running: true })
        .with_runtime(runtime)
        .event_handler({
            let lease = lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Wait),
                Event::Tick => Command::none(),
            }
        })
        .effect_handler(handle_effect)
        .subscription_handler(handle_subscriptions)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    let snapshot = runner.shell().snapshot();
    assert_eq!(snapshot.active_tasks.len(), 1);
    assert_eq!(snapshot.active_subscriptions.len(), 1);
    assert_eq!(snapshot.untracked_task_count, 0);
    assert_eq!(snapshot.deferred_event_count, 0);
    assert_eq!(snapshot.queued_command_count, 0);
    assert_eq!(snapshot.pending_error_count, 0);
    assert!(snapshot.in_flight_count >= 2);
    assert!(snapshot.last_termination.is_none());
}

#[test]
fn shell_snapshot_records_explicit_task_cancellation_reason() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stop,
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
            }
        })
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    runner.core().try_send(Event::Stop).unwrap();
    runner.step().unwrap();

    let snapshot = runner.shell().snapshot();
    assert!(snapshot.active_tasks.is_empty());
    assert!(matches!(
        snapshot.last_termination,
        Some(record)
            if record.target == ShellTerminationTarget::Task
                && record.reason
                    == ShellTerminationReason::Cancelled(
                        ShellCancellationReason::ExplicitCommand
                    )
                && record.lease_id.is_some()
    ));
}

#[test]
fn shell_snapshot_records_subscription_reconcile_cancellation_reason() {
    #[derive(Debug, Clone)]
    enum Event {
        Disable,
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Key {
        Clock,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Disable => {
                let running = Running::extract_mut(ctx);
                **running = false;
                Command::none()
            }
            Event::Tick => Command::none(),
        }
    }

    fn describe(running: &Running) -> Subscription<Event, Effect> {
        if !**running {
            return Subscription::none();
        }

        Subscription::every(Key::Clock, Duration::from_secs(60), Event::Tick)
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
        handle!(describe, ctx)
    }

    let (runtime, _clock) = syzygy::runtime::Runtime::manual().unwrap();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model { running: true })
        .with_runtime(runtime)
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build()
        .unwrap();

    runner.step().unwrap();
    assert_eq!(runner.shell().snapshot().active_subscriptions.len(), 1);

    runner.core().try_send(Event::Disable).unwrap();
    runner.step().unwrap();

    let snapshot = runner.shell().snapshot();
    assert!(snapshot.active_subscriptions.is_empty());
    assert!(matches!(
        snapshot.last_termination,
        Some(record)
            if record.target == ShellTerminationTarget::Subscription
                && record.reason
                    == ShellTerminationReason::Cancelled(
                        ShellCancellationReason::SubscriptionReconciled
                    )
                && record.subscription_key.is_some()
    ));
}

#[test]
fn shutdown_preserves_specific_subscription_termination_target() {
    #[derive(Debug, Clone)]
    enum Event {
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Key {
        Clock,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<Model>) -> Command<Event, Effect> {
        Command::none()
    }

    fn describe(running: &Running) -> Subscription<Event, Effect> {
        if !**running {
            return Subscription::none();
        }

        Subscription::every(Key::Clock, Duration::from_secs(60), Event::Tick)
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
        handle!(describe, ctx)
    }

    let (runtime, _clock) = syzygy::runtime::Runtime::manual().unwrap();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model { running: true })
        .with_runtime(runtime)
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build()
        .unwrap();

    runner.step().unwrap();
    assert_eq!(runner.shell().snapshot().active_subscriptions.len(), 1);

    runner.shutdown();

    let snapshot = runner.shell().snapshot();
    assert!(snapshot.active_subscriptions.is_empty());
    assert!(matches!(
        snapshot.last_termination,
        Some(record)
            if record.target == ShellTerminationTarget::Subscription
                && record.reason
                    == ShellTerminationReason::Cancelled(ShellCancellationReason::Shutdown)
                && record.subscription_key.is_some()
    ));
}

#[test]
fn shell_snapshot_records_process_completion_reason() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Done)]
        done: bool,
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process(capture_process_spec(256, 256), |_result| {
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
                Event::Start => Command::abortable(&lease, Effect::Run),
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
    runner.run_until(|core, _| core.model().done).unwrap();

    let snapshot = runner.shell().snapshot();
    assert!(snapshot.active_tasks.is_empty());
    assert!(matches!(
        snapshot.last_termination,
        Some(record)
            if record.target == ShellTerminationTarget::Process
                && record.reason == ShellTerminationReason::Completed
    ));
}

#[test]
fn trace_recorder_is_disabled_by_default() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Wait,
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async { Command::none() }),
        }
    }

    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler({
            let lease = lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Wait),
            }
        })
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    assert!(runner.shell().trace_snapshot().is_empty());
}

#[test]
fn trace_recorder_captures_ordered_core_effect_subscription_transitions() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Tick,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Wait,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Key {
        Clock,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
    }

    fn describe(running: &Running) -> Subscription<Event, Effect> {
        if !**running {
            return Subscription::none();
        }

        Subscription::every(Key::Clock, Duration::from_secs(60), Event::Tick)
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, Effect> {
        handle!(describe, ctx)
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Wait => Task::once(async { Command::none() }),
        }
    }

    let diagnostics = DiagnosticsConfig::default()
        .trace(ShellTraceConfig::default().enabled(true).max_entries(256));
    let (runtime, _clock) = syzygy::runtime::Runtime::manual().unwrap();
    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model { running: true })
        .with_runtime(runtime)
        .event_handler({
            let lease = lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Wait),
                Event::Tick => Command::none(),
            }
        })
        .with_syzygy_config(SyzygyConfig::default().diagnostics(diagnostics))
        .effect_handler(handle_effect)
        .subscription_handler(handle_subscriptions)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    let trace = runner.shell().trace_snapshot();
    assert!(
        !trace.is_empty(),
        "trace should capture transitions when enabled"
    );
    assert!(
        trace
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence),
        "trace sequence numbers must be strictly increasing"
    );

    let core_dispatch = trace
        .iter()
        .position(|entry| matches!(entry.event, ShellTraceEvent::CoreEventCommandDispatched))
        .expect("core command dispatch event should be traced");
    let abortable_step = trace
        .iter()
        .position(|entry| {
            matches!(
                entry.event,
                ShellTraceEvent::CommandStep(ShellTraceCommandStep::Abortable { .. })
            )
        })
        .expect("abortable command step should be traced");
    let task_spawned = trace
        .iter()
        .position(|entry| {
            matches!(
                entry.event,
                ShellTraceEvent::TaskSpawned {
                    kind: ShellTraceTaskKind::Future,
                    ..
                }
            )
        })
        .expect("task spawn should be traced");
    let subscription_start = trace
        .iter()
        .position(|entry| matches!(entry.event, ShellTraceEvent::SubscriptionStarted { .. }))
        .expect("subscription start should be traced");
    let reconcile = trace
        .iter()
        .position(|entry| {
            matches!(
                entry.event,
                ShellTraceEvent::SubscriptionsReconciled { changes: 1 }
            )
        })
        .expect("subscription reconciliation should be traced");
    let drained = trace
        .iter()
        .position(|entry| matches!(entry.event, ShellTraceEvent::ShellDrained { .. }))
        .expect("drain phase should be traced");

    assert!(core_dispatch < abortable_step);
    assert!(abortable_step < task_spawned);
    assert!(task_spawned < subscription_start);
    assert!(subscription_start < reconcile);
    assert!(reconcile < drained);
}

#[test]
fn trace_recorder_captures_process_control_steps() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process_interactive(
                interactive_echo_process_spec(bytes_framing()),
                |_update| None,
            ),
        }
    }

    let diagnostics = DiagnosticsConfig::default()
        .trace(ShellTraceConfig::default().enabled(true).max_entries(256));
    let lease = TaskLease::new();
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler({
            let lease = lease.clone();
            move |event, _ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Run)
                    .and_process_write(&lease, expected_line_bytes("alpha"))
                    .and_process_close_stdin(&lease),
            }
        })
        .with_syzygy_config(SyzygyConfig::default().diagnostics(diagnostics))
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    let trace = runner.shell().trace_snapshot();
    runner.shutdown();

    assert!(trace.iter().any(|entry| {
        matches!(
            entry.event,
            ShellTraceEvent::CommandStep(ShellTraceCommandStep::ProcessWrite { .. })
        )
    }));
    assert!(trace.iter().any(|entry| {
        matches!(
            entry.event,
            ShellTraceEvent::CommandStep(ShellTraceCommandStep::ProcessCloseStdin { .. })
        )
    }));
    assert!(trace
        .iter()
        .any(|entry| { matches!(entry.event, ShellTraceEvent::ProcessControl { .. }) }));
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
        #[model(wrapper = Ticks)]
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
        #[model(wrapper = Done)]
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
        #[model(wrapper = Done)]
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

#[test]
fn blocking_effects_resolve_on_syzygys_owned_runtime() {
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
        #[model(wrapper = Done)]
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
            Effect::Work => Task::blocking(|| {
                std::thread::sleep(Duration::from_millis(5));
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
    runner.run().unwrap();

    assert!(runner.model().done);
}

#[test]
fn cooperative_blocking_task_cancels_on_explicit_abort() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stop,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Done)]
        done: bool,
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_in_effect = Arc::clone(&cancelled);
    let lease = TaskLease::new();

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler({
            let lease = lease.clone();
            move |event, ctx| match event {
                Event::Start => Command::abortable(&lease, Effect::Work),
                Event::Stop => Command::cancel(&lease),
                Event::Done => {
                    let done = Done::extract_mut(ctx);
                    **done = true;
                    Command::none()
                }
            }
        })
        .effect_handler(move |effect, _ctx| {
            let cancelled_in_effect = Arc::clone(&cancelled_in_effect);
            match effect {
                Effect::Work => Task::blocking_cooperative(move |cancel| {
                    for _ in 0..100 {
                        if cancel.is_cancelled() {
                            cancelled_in_effect.store(true, Ordering::SeqCst);
                            return None;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }

                    Some(Command::event(Event::Done))
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.core().try_send(Event::Stop).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(cancelled.load(Ordering::SeqCst));
    assert!(!runner.model().done);
}

#[test]
fn cooperative_blocking_task_cancels_when_lease_owner_drops() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        DropLease,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Lease)]
        lease: AbortSlot,
        #[model(wrapper = Done)]
        done: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Lease::extract_mut(ctx).start(Effect::Work),
            Event::DropLease => {
                let current = Lease::extract_mut(ctx);
                current.clear();
                Command::none()
            }
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_in_effect = Arc::clone(&cancelled);

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(move |effect, _ctx| {
            let cancelled_in_effect = Arc::clone(&cancelled_in_effect);
            match effect {
                Effect::Work => Task::blocking_cooperative(move |cancel| {
                    for _ in 0..100 {
                        if cancel.is_cancelled() {
                            cancelled_in_effect.store(true, Ordering::SeqCst);
                            return None;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }

                    Some(Command::event(Event::Done))
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.core().try_send(Event::DropLease).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(cancelled.load(Ordering::SeqCst));
    assert!(!runner.model().done);
}

#[test]
fn deferred_event_overflow_errors_by_default_and_is_visible_in_snapshot() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Deferred,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    const OVERFLOW_EVENTS: usize = 70_000;

    fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::events((0..OVERFLOW_EVENTS).map(|_| Event::Deferred)),
            Event::Deferred => Command::none(),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .with_event_channel_capacity(Some(1))
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    let error = runner
        .step()
        .expect_err("overflow must surface as shell error");
    assert!(matches!(
        error,
        ShellError::DeferredEventOverflow {
            limit: 65_536,
            dropped_events: _
        }
    ));

    let snapshot = runner.shell().snapshot();
    assert!(snapshot.deferred_event_overflow_count > 0);
}

#[test]
fn deferred_event_overflow_drop_policy_keeps_running_and_records_trace() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Deferred,
    }

    #[derive(Debug, Clone)]
    enum Effect {}

    const OVERFLOW_EVENTS: usize = 70_000;

    fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::events((0..OVERFLOW_EVENTS).map(|_| Event::Deferred)),
            Event::Deferred => Command::none(),
        }
    }

    let diagnostics = DiagnosticsConfig::default()
        .deferred_event_overflow(DeferredEventOverflowPolicy::DropNewest)
        .trace(ShellTraceConfig::default().enabled(true));

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .with_event_channel_capacity(Some(1))
        .with_syzygy_config(SyzygyConfig::default().diagnostics(diagnostics))
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner
        .step()
        .expect("drop policy must not surface overflow as shell error");

    let snapshot = runner.shell().snapshot();
    assert!(snapshot.deferred_event_overflow_count > 0);

    let trace = runner.shell().trace_snapshot();
    assert!(trace.iter().any(|entry| {
        matches!(
            entry.event,
            ShellTraceEvent::DeferredEventOverflow {
                policy: DeferredEventOverflowPolicy::DropNewest,
                ..
            }
        )
    }));
}

#[test]
fn shell_is_not_idle_when_deferred_events_are_queued() {
    #[derive(Debug, Clone)]
    enum Event {
        A,
        B,
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
        .with_event_channel_capacity(Some(1))
        .build()
        .unwrap();

    runner
        .shell_mut()
        .dispatch_command(Command::event(Event::A).and_event(Event::B))
        .unwrap();

    assert_eq!(runner.shell().snapshot().deferred_event_count, 1);
    assert!(
        !runner.shell().is_idle(),
        "shell with deferred events must not report idle"
    );
}

#[test]
fn shell_is_not_idle_when_pending_errors_exist() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        TriggerInvalidControl,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        Command::effect(Effect::TriggerInvalidControl)
    }

    let resolve_invalid_command = Arc::new(AtomicBool::new(false));
    let resolve_invalid_command_in_effect = Arc::clone(&resolve_invalid_command);

    let handle_effect = move |effect: Effect, _ctx: &EffectContext<'_>| match effect {
        Effect::TriggerInvalidControl => {
            let resolve_invalid_command = Arc::clone(&resolve_invalid_command_in_effect);
            Task::once(async move {
                while !resolve_invalid_command.load(Ordering::SeqCst) {
                    syzygy::runtime::yield_now().await;
                }
                Command::process_close_stdin(TaskLease::new())
            })
        }
    };

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();

    resolve_invalid_command.store(true, Ordering::SeqCst);
    for _ in 0..32 {
        if runner.shell().has_pending_errors() {
            break;
        }
        runner.shell().runtime().drive_ready();
    }

    assert!(runner.shell().has_pending_errors());
    assert_eq!(runner.shell().snapshot().pending_error_count, 1);
    assert!(!runner.shell().is_idle());

    let error = runner
        .shell_mut()
        .drain()
        .expect_err("pending error must surface");
    assert!(matches!(error, ShellError::InvalidProcessControl(_)));
}

#[test]
fn run_until_returns_error_when_spawned_command_sets_pending_error() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        TriggerInvalidControl,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        Command::effect(Effect::TriggerInvalidControl)
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::TriggerInvalidControl => {
                Task::once(async { Command::process_close_stdin(TaskLease::new()) })
            }
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    let error = runner
        .run_until(|_, _| false)
        .expect_err("run_until must propagate pending shell errors");

    assert!(matches!(error, ShellError::InvalidProcessControl(_)));
}

#[test]
fn shutdown_waits_for_non_abortable_blocking_tasks() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Work,
    }

    let finished = Arc::new(AtomicBool::new(false));
    let finished_in_effect = Arc::clone(&finished);
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let release_barrier = Arc::new(std::sync::Barrier::new(2));
    let release_barrier_in_effect = Arc::clone(&release_barrier);

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(|event, _ctx| match event {
            Event::Start => Command::effect(Effect::Work),
        })
        .effect_handler(move |effect, _ctx| {
            let finished_in_effect = Arc::clone(&finished_in_effect);
            let release_barrier = Arc::clone(&release_barrier_in_effect);
            let started_tx = started_tx.clone();
            match effect {
                Effect::Work => Task::blocking(move || {
                    started_tx.send(()).unwrap();
                    release_barrier.wait();
                    std::thread::sleep(Duration::from_millis(20));
                    finished_in_effect.store(true, Ordering::SeqCst);
                    Command::none()
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    let shutdown_started = Instant::now();
    release_barrier.wait();
    runner.shutdown();

    assert!(finished.load(Ordering::SeqCst));
    assert!(
        shutdown_started.elapsed() >= Duration::from_millis(20),
        "shutdown should wait for non-abortable blocking work to finish"
    );
}

#[test]
fn spawned_abortable_blocking_violation_surfaces_as_shell_error() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Bootstrap,
        Invalid,
    }

    fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Bootstrap),
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Bootstrap => Task::once(async {
                let lease = TaskLease::new();
                Command::abortable(lease, Effect::Invalid)
            }),
            Effect::Invalid => Task::blocking(Command::none),
        }
    }

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    let err = runner.step().unwrap_err();

    assert_eq!(err, ShellError::AbortableBlockingTask);
}

#[test]
fn process_tasks_capture_bounded_output() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Done(ProcessExit),
        Failed(ProcessError),
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(part)]
        exit: Option<ProcessExit>,
        #[model(part)]
        failure: Option<ProcessError>,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Run),
            Event::Done(exit) => {
                *Option::<ProcessExit>::extract_mut(ctx) = Some(exit);
                Command::none()
            }
            Event::Failed(error) => {
                *Option::<ProcessError>::extract_mut(ctx) = Some(error);
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process(capture_process_spec(3, 2), |result| match result {
                Ok(exit) => Command::event(Event::Done(exit)),
                Err(error) => Command::event(Event::Failed(error)),
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
    runner.run().unwrap();

    assert!(runner.model().failure.is_none());
    let exit = runner
        .model()
        .exit
        .as_ref()
        .expect("expected process output");
    assert!(exit.status.success());

    let stdout = exit.stdout.as_ref().expect("expected stdout capture");
    assert_eq!(stdout.bytes.len(), 3);
    assert!(stdout.truncated);

    let stderr = exit.stderr.as_ref().expect("expected stderr capture");
    assert_eq!(stderr.bytes.len(), 2);
    assert!(stderr.truncated);
}

#[test]
fn abortable_process_kills_on_explicit_cancel() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stop,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Lease)]
        lease: AbortSlot,
        #[model(wrapper = Done)]
        done: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Lease::extract_mut(ctx).start(Effect::Run),
            Event::Stop => Lease::extract_mut(ctx).cancel(),
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    let output_path = unique_process_output_path("explicit-cancel");
    remove_process_output(&output_path);

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler({
            let output_path = output_path.clone();
            move |effect, _ctx| match effect {
                Effect::Run => Task::process(delayed_write_process_spec(&output_path), |_| {
                    Command::event(Event::Done)
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.core().try_send(Event::Stop).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();
    std::thread::sleep(process_kill_observation_delay());

    assert!(!runner.model().done);
    assert!(!output_path.exists());
}

#[test]
fn abortable_process_kills_when_lease_owner_drops() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        DropLease,
        Done,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Lease)]
        lease: AbortSlot,
        #[model(wrapper = Done)]
        done: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Lease::extract_mut(ctx).start(Effect::Run),
            Event::DropLease => {
                Lease::extract_mut(ctx).clear();
                Command::none()
            }
            Event::Done => {
                let done = Done::extract_mut(ctx);
                **done = true;
                Command::none()
            }
        }
    }

    let output_path = unique_process_output_path("owner-drop");
    remove_process_output(&output_path);

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler({
            let output_path = output_path.clone();
            move |effect, _ctx| match effect {
                Effect::Run => Task::process(delayed_write_process_spec(&output_path), |_| {
                    Command::event(Event::Done)
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.core().try_send(Event::DropLease).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();
    std::thread::sleep(process_kill_observation_delay());

    assert!(!runner.model().done);
    assert!(!output_path.exists());
}

#[test]
fn shutdown_kills_untracked_process_tasks() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Run),
        }
    }

    let output_path = unique_process_output_path("shutdown");
    remove_process_output(&output_path);

    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler({
            let output_path = output_path.clone();
            move |effect, _ctx| match effect {
                Effect::Run => Task::process(delayed_write_process_spec(&output_path), |_| {
                    Command::none()
                }),
            }
        })
        .build()
        .unwrap();

    runner.core().try_send(Event::Start).unwrap();
    runner.step().unwrap();
    runner.shutdown();
    std::thread::sleep(process_kill_observation_delay());

    assert!(!output_path.exists());
}

#[test]
fn interactive_process_accepts_same_command_start_write_close_and_streams_stdout() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stdout(ProcessFrame),
        Exited(Result<ProcessExit, ProcessError>),
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Job)]
        job: AbortSlot,
        #[model(wrapper = StdoutFrames)]
        stdout_frames: Vec<ProcessFrame>,
        #[model(part)]
        exit: Option<ProcessExit>,
        #[model(part)]
        failure: Option<ProcessError>,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => {
                let job = Job::extract_mut(ctx);
                job.start(Effect::Run)
                    .and(job.write(expected_line_bytes("alpha")))
                    .and(job.write(expected_line_bytes("beta")))
                    .and(job.close_stdin())
            }
            Event::Stdout(frame) => {
                StdoutFrames::extract_mut(ctx).push(frame);
                Command::none()
            }
            Event::Exited(result) => {
                Job::extract_mut(ctx).clear();
                match result {
                    Ok(exit) => {
                        *Option::<ProcessExit>::extract_mut(ctx) = Some(exit);
                    }
                    Err(error) => {
                        *Option::<ProcessError>::extract_mut(ctx) = Some(error);
                    }
                }
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process_interactive(
                interactive_echo_process_spec(ProcessFraming::Lines { max_line_bytes: 64 }),
                |update| match update {
                    ProcessUpdate::Stdout(frame) => Some(Command::event(Event::Stdout(frame))),
                    ProcessUpdate::Exited(result) => Some(Command::event(Event::Exited(result))),
                    ProcessUpdate::Stderr(_) => None,
                },
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
    runner.run().unwrap();

    assert!(runner.model().failure.is_none());
    assert!(runner
        .model()
        .exit
        .as_ref()
        .is_some_and(|exit| exit.status.success()));
    assert_eq!(
        runner.model().stdout_frames,
        vec![
            ProcessFrame::Line(expected_line_bytes("alpha")),
            ProcessFrame::Line(expected_line_bytes("beta")),
        ]
    );
}

#[test]
fn interactive_process_write_after_close_returns_shell_error() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Close,
        WriteAfterClose,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Job)]
        job: AbortSlot,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        let job = Job::extract_mut(ctx);
        match event {
            Event::Start => job.start(Effect::Run),
            Event::Close => job.close_stdin(),
            Event::WriteAfterClose => job.write(expected_line_bytes("late")),
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process_interactive(
                interactive_echo_process_spec(bytes_framing()),
                |_update| None,
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
    runner.core().try_send(Event::Close).unwrap();
    runner.step().unwrap();
    runner.core().try_send(Event::WriteAfterClose).unwrap();
    let err = runner.step().unwrap_err();
    runner.shutdown();

    assert!(matches!(
        err,
        ShellError::InvalidProcessControl(message) if message.contains("stdin is already closed")
    ));
}

#[test]
fn interactive_process_line_limit_surfaces_error() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Exited(Result<ProcessExit, ProcessError>),
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Job)]
        job: AbortSlot,
        #[model(part)]
        exit: Option<ProcessExit>,
        #[model(part)]
        failure: Option<ProcessError>,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => {
                let job = Job::extract_mut(ctx);
                job.start(Effect::Run)
                    .and(job.write(expected_line_bytes("toolong")))
                    .and(job.close_stdin())
            }
            Event::Exited(result) => {
                Job::extract_mut(ctx).clear();
                match result {
                    Ok(exit) => {
                        *Option::<ProcessExit>::extract_mut(ctx) = Some(exit);
                    }
                    Err(error) => {
                        *Option::<ProcessError>::extract_mut(ctx) = Some(error);
                    }
                }
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process_interactive(
                interactive_echo_process_spec(ProcessFraming::Lines { max_line_bytes: 3 }),
                |update| match update {
                    ProcessUpdate::Exited(result) => Some(Command::event(Event::Exited(result))),
                    ProcessUpdate::Stdout(_) | ProcessUpdate::Stderr(_) => None,
                },
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
    runner.run().unwrap();

    assert!(runner.model().exit.is_none());
    assert_eq!(
        runner.model().failure.as_ref().map(|error| error.kind),
        Some(ProcessErrorKind::StdoutFrameTooLong)
    );
}

#[test]
fn cancelled_interactive_process_emits_no_terminal_update() {
    #[derive(Debug, Clone)]
    enum Event {
        Start,
        Stop,
        Exited,
    }

    #[derive(Debug, Clone)]
    enum Effect {
        Run,
    }

    #[derive(Debug, Default, Model)]
    struct Model {
        #[model(wrapper = Job)]
        job: AbortSlot,
        #[model(wrapper = Exited)]
        exited: bool,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Job::extract_mut(ctx).start(Effect::Run),
            Event::Stop => Job::extract_mut(ctx).cancel(),
            Event::Exited => {
                let exited = Exited::extract_mut(ctx);
                **exited = true;
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process_interactive(
                interactive_echo_process_spec(bytes_framing()),
                |update| match update {
                    ProcessUpdate::Exited(_) => Some(Command::event(Event::Exited)),
                    ProcessUpdate::Stdout(_) | ProcessUpdate::Stderr(_) => None,
                },
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
    runner.core().try_send(Event::Stop).unwrap();
    runner.step().unwrap();
    runner.run_until(|_, shell| shell.is_idle()).unwrap();

    assert!(!runner.model().exited);
}

#[test]
fn builder_uses_injected_runtime_before_event_handler() {
    let runtime = syzygy::runtime::Runtime::new().unwrap();

    let runner = Syzygy::builder::<(), ()>()
        .with_runtime(runtime.clone())
        .model(())
        .event_handler(|(), _ctx| Command::<(), ()>::none())
        .build()
        .unwrap();

    assert!(runner.shell().runtime().ptr_eq(&runtime));
}

#[test]
fn builder_uses_injected_runtime_after_event_handler() {
    let runtime = syzygy::runtime::Runtime::new().unwrap();

    let runner = Syzygy::builder::<(), ()>()
        .model(())
        .event_handler(|(), _ctx| Command::<(), ()>::none())
        .with_runtime(runtime.clone())
        .build()
        .unwrap();

    assert!(runner.shell().runtime().ptr_eq(&runtime));
}

static NEXT_PROCESS_OUTPUT_ID: AtomicU64 = AtomicU64::new(1);

fn unique_process_output_path(label: &str) -> std::path::PathBuf {
    let id = NEXT_PROCESS_OUTPUT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "syzygy-process-{label}-{id}-{}.tmp",
        std::process::id()
    ))
}

fn remove_process_output(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => panic!("failed to remove test output {}: {err}", path.display()),
    }
}

fn process_kill_observation_delay() -> Duration {
    #[cfg(windows)]
    {
        Duration::from_millis(1_500)
    }

    #[cfg(not(windows))]
    {
        Duration::from_millis(500)
    }
}

fn bytes_framing() -> ProcessFraming {
    ProcessFraming::Bytes {
        max_chunk_bytes: 256,
    }
}

fn expected_line_bytes(line: &str) -> Vec<u8> {
    #[cfg(windows)]
    {
        format!("{line}\r\n").into_bytes()
    }

    #[cfg(not(windows))]
    {
        format!("{line}\n").into_bytes()
    }
}

fn interactive_echo_process_spec(stdout_framing: ProcessFraming) -> ProcessSpec {
    let spec = ProcessSpec::new(interactive_echo_program())
        .args(interactive_echo_args())
        .stdin(ProcessInput::Piped)
        .stdout(ProcessOutput::Stream {
            framing: stdout_framing,
        })
        .stderr(ProcessOutput::Discard)
        .termination_policy(ProcessTerminationPolicy::CloseStdinThenKill {
            grace: Duration::from_millis(50),
        });

    #[cfg(windows)]
    {
        spec
    }

    #[cfg(not(windows))]
    {
        spec
    }
}

#[cfg(windows)]
fn interactive_echo_program() -> &'static str {
    "cmd"
}

#[cfg(not(windows))]
fn interactive_echo_program() -> &'static str {
    "sh"
}

#[cfg(windows)]
fn interactive_echo_args() -> [&'static str; 3] {
    ["/Q", "/C", "more"]
}

#[cfg(not(windows))]
fn interactive_echo_args() -> [&'static str; 2] {
    ["-c", "cat"]
}

fn capture_process_spec(stdout_limit: usize, stderr_limit: usize) -> ProcessSpec {
    #[cfg(windows)]
    {
        ProcessSpec::new("cmd")
            .args(["/C", "echo stdout-data & echo stderr-data 1>&2"])
            .stdout(ProcessOutput::Capture {
                max_bytes: stdout_limit,
            })
            .stderr(ProcessOutput::Capture {
                max_bytes: stderr_limit,
            })
    }

    #[cfg(not(windows))]
    {
        ProcessSpec::new("sh")
            .args(["-c", "printf stdout-data; printf stderr-data >&2"])
            .stdout(ProcessOutput::Capture {
                max_bytes: stdout_limit,
            })
            .stderr(ProcessOutput::Capture {
                max_bytes: stderr_limit,
            })
    }
}

fn delayed_write_process_spec(path: &Path) -> ProcessSpec {
    #[cfg(windows)]
    {
        let script = format!(
            "ping 127.0.0.1 -n 3 >NUL && echo done>\"{}\"",
            path.to_string_lossy()
        );

        ProcessSpec::new("cmd").args(["/C", script.as_str()])
    }

    #[cfg(not(windows))]
    {
        ProcessSpec::new("sh")
            .arg("-c")
            .arg("sleep 0.3; printf done > \"$1\"")
            .arg("syzygy-process")
            .arg(path.as_os_str().to_os_string())
    }
}
