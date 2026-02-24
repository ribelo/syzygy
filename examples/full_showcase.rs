use std::time::Duration;

use futures::stream;
use futures::StreamExt;
use syzygy::prelude::*;

// -- Child model: extracted via #[extract], no wrapper types needed --

#[derive(Debug, Default, Model)]
struct Stats {
    ticks: i32,
    saves: i32,
}

// -- Parent model: uses #[extract] for child and wrappers for primitives --

#[derive(Debug, Default, Model)]
struct AppState {
    counter: i32,
    status: String,
    #[extract]
    stats: Stats,
}

// -- Events --

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Increment(i32),
    Save,
    SaveDone(i32),
    Tick,
    SetStatus(String),
    StreamFinished,
}

// -- Effects --

#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    SaveToServer(i32),
    StartTicker,
    SlowCompute(i32),
}

// -- Resources (Rc, not Arc -- single-threaded) --

#[derive(Debug, Clone)]
struct ServerUrl(String);

// -- Event handlers: mix of wrapper extractors and &mut child --

fn increment(amount: i32, mut counter: Counter) -> Command<Event, Effect> {
    *counter += amount;
    Command::none()
}

fn save(counter: Counter) -> Command<Event, Effect> {
    Command::effect(Effect::SaveToServer(*counter))
}

fn record_save(stats: &mut Stats) -> Command<Event, Effect> {
    stats.saves += 1;
    Command::none()
}

fn save_done(value: i32, mut status: Status) -> Command<Event, Effect> {
    *status = format!("saved: {value}");
    Command::none()
}

fn tick(mut counter: Counter) -> Command<Event, Effect> {
    *counter += 1;
    Command::none()
}

fn record_tick(stats: &mut Stats) -> Command<Event, Effect> {
    stats.ticks += 1;
    Command::none()
}

fn set_status(new_status: String, mut status: Status) -> Command<Event, Effect> {
    *status = new_status;
    Command::none()
}

fn stream_finished(mut status: Status) -> Command<Event, Effect> {
    *status = "stream done".to_string();
    Command::none()
}

fn dispatch_event(event: Event, ctx: &EventContext<AppState>) -> Command<Event, Effect> {
    match event {
        Event::Increment(n) => increment.handle(n, ctx),
        Event::Save => {
            let effect_cmd = save.handle((), ctx);
            let stats_cmd = record_save.handle((), ctx);
            effect_cmd.and(stats_cmd)
        }
        Event::SaveDone(v) => save_done.handle(v, ctx),
        Event::Tick => {
            let counter_cmd = tick.handle((), ctx);
            let stats_cmd = record_tick.handle((), ctx);
            counter_cmd.and(stats_cmd)
        }
        Event::SetStatus(s) => set_status.handle(s, ctx),
        Event::StreamFinished => stream_finished.handle((), ctx),
    }
}

// -- Effect handlers: async fns and stream-returning fns --

async fn save_to_server(value: i32, url: Res<ServerUrl>) -> Command<Event, Effect> {
    let _endpoint = format!("{}/save?v={value}", url.0);
    compio::runtime::time::sleep(Duration::from_millis(1)).await;
    Command::event(Event::SaveDone(value))
}

fn start_ticker(_: ()) -> impl futures::Stream<Item = Command<Event, Effect>> {
    stream::iter(vec![
        Command::event(Event::Tick),
        Command::event(Event::Tick),
        Command::event(Event::Tick),
    ])
    .chain(stream::once(async {
        Command::event(Event::StreamFinished)
    }))
}

async fn slow_compute(value: i32) -> Command<Event, Effect> {
    let result = compio::runtime::spawn_blocking(move || value * value)
        .await
        .expect("spawn_blocking should not panic");
    Command::event(Event::SetStatus(format!("computed: {result}")))
}

fn dispatch_effect(effect: Effect, ctx: &EffectContext) -> Task<Event, Effect> {
    match effect {
        Effect::SaveToServer(v) => save_to_server.handle(v, ctx),
        Effect::StartTicker => start_ticker.handle((), ctx),
        Effect::SlowCompute(v) => slow_compute.handle(v, ctx),
    }
}

// -- Main: compio single-threaded runtime --

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppState::default())
        .with_resource(ServerUrl("https://api.example.test".to_string()))
        .event_handler(dispatch_event)
        .effect_handler(dispatch_effect)
        .build();

    runner.core().try_send_event(Event::Increment(5))?;
    runner.step()?;
    assert_eq!(runner.model().counter, 5);

    runner.core().try_send_event(Event::Save)?;
    runner.step()?;
    runner.step()?;
    assert_eq!(runner.model().stats.saves, 1);
    assert_eq!(runner.model().status, "saved: 5");

    runner
        .shell_mut()
        .dispatch_command(Command::effect(Effect::StartTicker))?;
    for _ in 0..5 {
        runner.step()?;
    }
    assert_eq!(runner.model().stats.ticks, 3);
    assert_eq!(runner.model().counter, 8);

    runner
        .shell_mut()
        .dispatch_command(Command::effect(Effect::SlowCompute(7)))?;
    runner.step()?;
    runner.step()?;
    assert_eq!(runner.model().status, "computed: 49");

    println!(
        "counter={} status={} ticks={} saves={}",
        runner.model().counter,
        runner.model().status,
        runner.model().stats.ticks,
        runner.model().stats.saves,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_updates_counter() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);
        store.send(Event::Increment(3));
        assert_eq!(store.state().counter, 3);
        store.assert_no_effects();
    }

    #[test]
    fn save_increments_stats_and_emits_effect() {
        let mut store = TestStore::new(
            AppState {
                counter: 10,
                ..Default::default()
            },
            dispatch_event,
        );
        store.send(Event::Save);
        assert_eq!(store.state().stats.saves, 1);
        store.assert_effects([Effect::SaveToServer(10)]);
    }

    #[test]
    fn tick_increments_both() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);
        store.send(Event::Tick);
        assert_eq!(store.state().counter, 1);
        assert_eq!(store.state().stats.ticks, 1);
        store.assert_no_effects();
    }

    #[test]
    fn save_done_sets_status() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);
        store.send(Event::SaveDone(42));
        assert_eq!(store.state().status, "saved: 42");
    }

    #[test]
    fn async_effect_produces_event() {
        let rt = compio::runtime::Runtime::new().expect("compio runtime");
        rt.block_on(async {
            let mut resources = ResourceMap::new();
            resources.insert(ServerUrl("https://test.example".to_string()));
            let ctx = EffectContext::new(resources);
            let task = save_to_server.handle(99, &ctx);
            match task {
                Task::Once(fut) => {
                    let cmd = fut.await;
                    let steps: Vec<_> = cmd.into_iter().collect();
                    assert_eq!(steps.len(), 1);
                    match &steps[0] {
                        CommandStep::Event(Event::SaveDone(99)) => {}
                        other => panic!("unexpected step: {other:?}"),
                    }
                }
                _ => panic!("expected Task::Once"),
            }
        });
    }

    #[test]
    fn stream_effect_produces_events() {
        let task = start_ticker.handle((), &EffectContext::new(ResourceMap::new()));
        match task {
            Task::Stream(mut stream) => {
                let rt = compio::runtime::Runtime::new().expect("compio runtime");
                rt.block_on(async {
                    let mut events = Vec::new();
                    while let Some(cmd) = stream.next().await {
                        for step in cmd {
                            if let CommandStep::Event(e) = step {
                                events.push(e);
                            }
                        }
                    }
                    assert_eq!(
                        events,
                        vec![Event::Tick, Event::Tick, Event::Tick, Event::StreamFinished]
                    );
                });
            }
            _ => panic!("expected Task::Stream"),
        }
    }
}
