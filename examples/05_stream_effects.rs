use futures::stream::{self, StreamExt};
use std::time::Duration;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    ticks: i32,
    active_timers: i32,
    timer_1: Option<TaskLease>,
    timer_2: Option<TaskLease>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    StartTimer(u32),
    StopTimer(u32),
    Tick,
    TimerFinished(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    SpawnTicker(u32),
}

/// ## Why use `TaskLease`?
/// `TaskLease` gives the timer an explicit owner in model state.
/// As long as the lease is retained, the stream is allowed to keep running.
fn start_timer(
    id: u32,
    active_timers: &mut ActiveTimers,
    timer_1: &mut Timer1,
    timer_2: &mut Timer2,
) -> Command<AppEvent, AppEffect> {
    **active_timers += 1;
    let lease = TaskLease::new();
    match id {
        1 => **timer_1 = Some(lease.clone()),
        2 => **timer_2 = Some(lease.clone()),
        _ => {}
    }
    Command::abortable(lease, AppEffect::SpawnTicker(id))
}

/// ## Why use explicit cancel?
/// We stop the stream explicitly and drop its lease from model state.
fn stop_timer(
    id: u32,
    active_timers: &mut ActiveTimers,
    timer_1: &mut Timer1,
    timer_2: &mut Timer2,
) -> Command<AppEvent, AppEffect> {
    **active_timers -= 1;
    let lease = match id {
        1 => timer_1.take(),
        2 => timer_2.take(),
        _ => None,
    };

    lease.map_or_else(Command::none, Command::cancel)
}

fn tick(_: (), ticks: &mut Ticks) -> Command<AppEvent, AppEffect> {
    **ticks += 1;
    Command::none()
}

fn timer_finished(
    id: u32,
    active_timers: &mut ActiveTimers,
    timer_1: &mut Timer1,
    timer_2: &mut Timer2,
) -> Command<AppEvent, AppEffect> {
    **active_timers -= 1;
    match id {
        1 => {
            let _ = timer_1.take();
        }
        2 => {
            let _ = timer_2.take();
        }
        _ => {}
    }
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::StartTimer(id) => handle!(start_timer, ctx, id),
        AppEvent::StopTimer(id) => handle!(stop_timer, ctx, id),
        AppEvent::Tick => handle!(tick, ctx),
        AppEvent::TimerFinished(id) => handle!(timer_finished, ctx, id),
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
    .chain(stream::once(async move {
        Command::event(AppEvent::TimerFinished(id))
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
        .build()?;

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
