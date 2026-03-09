use std::time::Duration;

use futures::stream;
use futures::StreamExt;
use syzygy::prelude::*;

#[derive(Clone)]
struct PulseDriver;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PulseSpec {
    interval: Duration,
}

impl SubscriptionDriver for PulseDriver {
    type Spec = PulseSpec;
    type Update = ();

    fn subscribe(&self, spec: Self::Spec) -> SubscriptionStream<Self::Update> {
        stream::unfold(spec.interval, |interval| async move {
            syzygy::runtime::sleep(interval).await;
            Some(((), interval))
        })
        .boxed_local()
    }
}

#[derive(Clone)]
struct RuntimeCheckedDriver;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeCheckedSpec;

impl SubscriptionDriver for RuntimeCheckedDriver {
    type Spec = RuntimeCheckedSpec;
    type Update = ();

    fn subscribe(&self, _spec: Self::Spec) -> SubscriptionStream<Self::Update> {
        #[cfg(feature = "rt-compio")]
        compio::runtime::Runtime::with_current(|_| ());

        #[cfg(feature = "rt-tokio")]
        {
            let _ = tokio::runtime::Handle::try_current()
                .expect("subscription driver should be constructed inside the owned runtime");
        }

        stream::once(async {}).boxed_local()
    }
}

#[test]
fn every_subscription_ticks_until_model_disables_it() {
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

        Subscription::every(Key::Clock, Duration::from_millis(1), Event::Tick)
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, ()> {
        match event {
            Event::Tick => handle!(on_tick, ctx),
        }
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut app = Syzygy::builder::<Event, ()>()
        .model(Model {
            running: true,
            ticks: 0,
        })
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
        .build()
        .unwrap();

    app.run_until(|core, _| !core.model().running).unwrap();

    assert_eq!(app.model().ticks, 2);
}

#[test]
fn subscription_mapper_updates_without_restarting_driver() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Pulse(String),
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Main,
    }

    #[derive(Default, Model)]
    struct Model {
        #[model(wrapper = Running)]
        running: bool,
        #[model(wrapper = Prefix)]
        prefix: String,
        #[model(wrapper = Count)]
        count: u8,
        #[model(wrapper = Last)]
        last: String,
    }

    fn on_pulse(
        message: String,
        running: &mut Running,
        prefix: &mut Prefix,
        count: &mut Count,
        last: &mut Last,
    ) -> Command<Event, ()> {
        **last = message;
        **count = count.saturating_add(1);

        if **count == 1 {
            **prefix = "B".into();
        }
        if **count >= 2 {
            **running = false;
        }

        Command::none()
    }

    fn describe(running: &Running, prefix: &Prefix) -> Subscription<Event, ()> {
        if !**running {
            return Subscription::none();
        }

        let prefix = prefix.to_string();
        Subscription::custom::<PulseDriver, _, _>(
            Key::Main,
            PulseSpec {
                interval: Duration::from_millis(1),
            },
            move |()| Some(Command::event(Event::Pulse(prefix.clone()))),
        )
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, ()> {
        match event {
            Event::Pulse(message) => handle!(on_pulse, ctx, message),
        }
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut app = Syzygy::builder::<Event, ()>()
        .model(Model {
            running: true,
            prefix: "A".into(),
            count: 0,
            last: String::new(),
        })
        .with_subscription_driver(PulseDriver)
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
        .build()
        .unwrap();

    app.run_until(|core, _| !core.model().running).unwrap();

    assert_eq!(app.model().count, 2);
    assert_eq!(app.model().last, "B");
}

#[test]
fn missing_custom_subscription_driver_returns_shell_error() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {}

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Main,
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

        Subscription::custom::<PulseDriver, _, _>(
            Key::Main,
            PulseSpec {
                interval: Duration::from_millis(1),
            },
            |()| None,
        )
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut app = Syzygy::builder::<Event, ()>()
        .model(Model { active: true })
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build()
        .unwrap();

    let error = app.step().unwrap_err();
    assert!(matches!(error, ShellError::MissingSubscriptionDriver(_)));
}

#[test]
fn duplicate_subscription_keys_are_rejected() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Tick,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        Shared,
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

        Subscription::batch([
            Subscription::every(Key::Shared, Duration::from_millis(1), Event::Tick),
            Subscription::every(Key::Shared, Duration::from_millis(1), Event::Tick),
        ])
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut app = Syzygy::builder::<Event, ()>()
        .model(Model { active: true })
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build()
        .unwrap();

    let error = app.step().unwrap_err();
    assert!(matches!(error, ShellError::DuplicateSubscriptionKey(_)));
}

#[test]
fn custom_subscription_driver_is_constructed_inside_owned_runtime() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Ready,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Key {
        RuntimeChecked,
    }

    #[derive(Model)]
    struct Model {
        #[model(wrapper = Active)]
        active: bool,
    }

    fn ready(active: &mut Active) -> Command<Event, ()> {
        **active = false;
        Command::none()
    }

    fn describe(active: &Active) -> Subscription<Event, ()> {
        if !**active {
            return Subscription::none();
        }

        Subscription::custom::<RuntimeCheckedDriver, _, _>(
            Key::RuntimeChecked,
            RuntimeCheckedSpec,
            |()| Some(Command::event(Event::Ready)),
        )
    }

    fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, ()> {
        match event {
            Event::Ready => handle!(ready, ctx),
        }
    }

    fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
        handle!(describe, ctx)
    }

    let mut app = Syzygy::builder::<Event, ()>()
        .model(Model { active: true })
        .with_subscription_driver(RuntimeCheckedDriver)
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
        .build()
        .unwrap();

    app.run_until(|core, _| !core.model().active).unwrap();
    assert!(!app.model().active);
}
