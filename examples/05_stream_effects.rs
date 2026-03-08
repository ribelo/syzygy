use futures::stream::{self, StreamExt};
use std::time::Duration;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    ticks: i32,
    active_timers: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    StartTimer(u32),
    StopTimer(u32),
    Tick,
    TimerFinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    SpawnTicker(u32),
}

/// ## Why use `track`?
/// `Command::track` assigns the `CancelId` to the asynchronous stream.
/// When dealing with ongoing streams, we need a way to stop them selectively.
fn start_timer(id: u32, active_timers: &mut ActiveTimers) -> Command<AppEvent, AppEffect> {
    **active_timers += 1;
    Command::track(id, AppEffect::SpawnTicker(id))
}

/// ## Why use `cancel`?
/// We stop the stream by emitting `Command::cancel` targeting the exact `CancelId`
/// we used when tracking.
fn stop_timer(id: u32, active_timers: &mut ActiveTimers) -> Command<AppEvent, AppEffect> {
    **active_timers -= 1;
    Command::cancel(id)
}

fn tick(_: (), ticks: &mut Ticks) -> Command<AppEvent, AppEffect> {
    **ticks += 1;
    Command::none()
}

fn timer_finished(_: (), active_timers: &mut ActiveTimers) -> Command<AppEvent, AppEffect> {
    **active_timers -= 1;
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::StartTimer(id) => handle!(start_timer, ctx, id),
        AppEvent::StopTimer(id) => handle!(stop_timer, ctx, id),
        AppEvent::Tick => handle!(tick, ctx),
        AppEvent::TimerFinished => handle!(timer_finished, ctx),
    }
}

/// ## Why return an `impl Stream`?
///
/// Functions returning types that implement `Stream<Item = Command>` are automatically
/// recognized by Syzygy as `Task::Stream`. The shell will await each item in the
/// stream and process the resulting commands indefinitely.
fn spawn_ticker(id: u32) -> impl futures::Stream<Item = Command<AppEvent, AppEffect>> {
    stream::iter(vec![
        Command::event(AppEvent::Tick),
        Command::event(AppEvent::Tick),
        Command::event(AppEvent::Tick),
    ])
    .then(move |cmd| async move {
        println!("Timer {id} tick emitted.");
        syzygy::runtime::sleep(Duration::from_millis(5)).await;
        cmd
    })
    .chain(stream::once(async {
        Command::event(AppEvent::TimerFinished)
    }))
}

fn handle_effect(effect: AppEffect, ctx: &EffectContext<'_>) -> Task<AppEvent, AppEffect> {
    match effect {
        // `handle!` resolves to `Task::stream(...)` internally
        AppEffect::SpawnTicker(id) => handle!(spawn_ticker, ctx, id),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = AppEvent::StopTimer(0);

    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build();

    // Start multiple tickers concurrently
    app.core().try_send(AppEvent::StartTimer(1))?;
    app.core().try_send(AppEvent::StartTimer(2))?;

    // Let them run until both timers finish.
    app.run_until(|core, _| core.model().active_timers == 0)?;

    println!("Total ticks: {}", app.model().ticks);
    // 2 timers, 3 ticks each = 6 ticks
    assert_eq!(app.model().ticks, 6);

    Ok(())
}
