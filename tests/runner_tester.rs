use std::time::Duration;

use futures::StreamExt;
use syzygy::prelude::*;

#[test]
fn runner_tester_advances_manual_time_for_subscriptions() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Tick,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Clock,
    }

    #[derive(Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
        #[model(wrapper = Ticks)]
        ticks: u8,
    }

    fn on_tick(running: &mut Running, ticks: &mut Ticks) -> Command<Event, ()> {
        **ticks = ticks.saturating_add(1);
        if **ticks >= 2 {
            **running = false;
        }
        Command::none()
    }

    fn describe(running: &Running) -> Subscription<Event, ()> {
        if !**running {
            return Subscription::none();
        }

        Subscription::every(Key::Clock, Duration::from_secs(60), Event::Tick)
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, ()> {
        match event {
            Event::Tick => handle!(on_tick, ctx),
        }
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut tester = Syzygy::builder::<Event, ()>()
        .model(Model {
            running: true,
            ticks: 0,
        })
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build_tester()
        .unwrap();

    tester.drain().unwrap();
    assert_eq!(tester.now(), Duration::ZERO);
    assert_eq!(tester.state().ticks, 0);

    tester.advance_time(Duration::from_secs(60));
    tester.drain().unwrap();
    assert_eq!(tester.state().ticks, 1);

    tester.advance_time(Duration::from_secs(60));
    tester.drain().unwrap();
    assert_eq!(tester.state().ticks, 2);
    assert!(!tester.state().running);
}

#[test]
fn runner_tester_surfaces_shell_errors() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {}

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Missing,
    }

    #[derive(Clone)]
    struct MissingDriver;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MissingSpec;

    impl SubscriptionDriver for MissingDriver {
        type Spec = MissingSpec;
        type Update = ();

        fn subscribe(&self, _spec: Self::Spec) -> SubscriptionStream<Self::Update> {
            futures::stream::pending().boxed_local()
        }
    }

    #[derive(Model)]
    struct Model {
        #[model(wrapper = Active)]
        active: bool,
    }

    fn handle_event(_event: Event, _ctx: &EventContext<Model>) -> Command<Event, ()> {
        Command::none()
    }

    fn describe(active: &Active) -> Subscription<Event, ()> {
        if !**active {
            return Subscription::none();
        }

        Subscription::custom::<MissingDriver, _, _>(Key::Missing, MissingSpec, |()| None)
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut tester = Syzygy::builder::<Event, ()>()
        .model(Model { active: true })
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build_tester()
        .unwrap();

    let error = tester.step().unwrap_err();
    assert!(matches!(error, ShellError::MissingSubscriptionDriver(_)));
}

#[test]
fn runner_tester_waits_for_process_tasks() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Start,
        Finished(String),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Effect {
        Run,
    }

    #[derive(Default, Model)]
    struct Model {
        #[model(wrapper = Output)]
        output: String,
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
        match event {
            Event::Start => Command::effect(Effect::Run),
            Event::Finished(output) => {
                let state = Output::extract_mut(ctx);
                **state = output;
                Command::none()
            }
        }
    }

    fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
        match effect {
            Effect::Run => Task::process(capture_process_spec(128, 128), |result| match result {
                Ok(exit) => {
                    let stdout = exit.stdout.expect("stdout should be captured");
                    let output = String::from_utf8(stdout.bytes).expect("stdout should be utf-8");
                    Command::event(Event::Finished(output))
                }
                Err(error) => panic!("process task failed unexpectedly: {error}"),
            }),
        }
    }

    let mut tester = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build_tester()
        .unwrap();

    tester.send(Event::Start);
    let completed = tester
        .wait_for(Duration::from_secs(1), |runner| {
            !runner.model().output.is_empty()
        })
        .unwrap();
    assert!(completed, "process task did not complete before timeout");
    assert_eq!(tester.state().output, "stdout-data");
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
