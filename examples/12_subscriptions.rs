use std::time::Duration;

use syzygy::prelude::*;

#[derive(Clone, Debug)]
enum Event {
    Tick,
    WeatherLoaded(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SubKey {
    Clock,
    Weather,
}

#[derive(Default, Model)]
struct Model {
    #[model(wrapper = ClockRunning)]
    clock_running: bool,
    #[model(wrapper = WeatherPolling)]
    weather_polling: bool,
    #[model(wrapper = Ticks)]
    ticks: u8,
    #[model(wrapper = LastWeather)]
    last_weather: String,
}

#[derive(Clone)]
struct WeatherPollDriver;

#[derive(Clone, Debug, PartialEq, Eq)]
struct WeatherPollSpec {
    every: Duration,
}

impl SubscriptionDriver for WeatherPollDriver {
    type Spec = WeatherPollSpec;
    type Update = String;

    fn subscribe(&self, spec: Self::Spec) -> SubscriptionStream<Self::Update> {
        let interval = spec.every;
        Self::poll(spec, interval, |_spec| async move {
            "{\"temp_c\":21,\"summary\":\"clear\"}".to_owned()
        })
    }
}

fn tick(clock_running: &mut ClockRunning, ticks: &mut Ticks) -> Command<Event, ()> {
    let next = (**ticks).saturating_add(1);
    **ticks = next;

    if next >= 3 {
        **clock_running = false;
    }

    Command::none()
}

fn weather_loaded(
    payload: String,
    weather_polling: &mut WeatherPolling,
    last_weather: &mut LastWeather,
) -> Command<Event, ()> {
    **last_weather = payload;
    **weather_polling = false;
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, ()> {
    match event {
        Event::Tick => handle!(tick, ctx),
        Event::WeatherLoaded(payload) => handle!(weather_loaded, ctx, payload),
    }
}

fn describe_subscriptions(
    clock_running: &ClockRunning,
    weather_polling: &WeatherPolling,
) -> Subscription<Event, ()> {
    let mut subscriptions = Vec::new();

    if **clock_running {
        subscriptions.push(Subscription::every(
            SubKey::Clock,
            Duration::from_millis(250),
            Event::Tick,
        ));
    }

    if **weather_polling {
        subscriptions.push(Subscription::custom::<WeatherPollDriver, _, _>(
            SubKey::Weather,
            WeatherPollSpec {
                every: Duration::from_secs(60 * 60),
            },
            |json| Some(Command::event(Event::WeatherLoaded(json))),
        ));
    }

    Subscription::batch(subscriptions)
}

fn handle_subscriptions(ctx: &SubscriptionContext<Model>) -> Subscription<Event, ()> {
    handle!(describe_subscriptions, ctx)
}

fn main() -> Result<(), ShellError> {
    let mut app = Syzygy::builder::<Event, ()>()
        .model(Model {
            clock_running: true,
            weather_polling: true,
            ticks: 0,
            last_weather: String::new(),
        })
        .with_subscription_driver(WeatherPollDriver)
        .event_handler(handle_event)
        .subscription_handler(handle_subscriptions)
        .build()?;

    app.run_until(|core, _| !core.model().clock_running && !core.model().weather_polling)?;
    println!("ticks: {}", app.model().ticks);
    println!("last weather: {}", app.model().last_weather);
    Ok(())
}
