use futures::stream::{self, StreamExt};
use std::time::Duration;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Ticks)]
    ticks: i32,
    #[model(wrapper = ActiveTimers)]
    active_timers: i32,
    #[model(wrapper = Timer1)]
    timer_1: AbortSlot,
    #[model(wrapper = Timer2)]
    timer_2: AbortSlot,
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

/// ## Why use `AbortSlot`?
/// `AbortSlot` keeps timer ownership in model state and builds the right
/// command when a timer starts, restarts, or stops.
fn start_timer(
    id: u32,
    active_timers: &mut ActiveTimers,
    timer_1: &mut Timer1,
    timer_2: &mut Timer2,
) -> Command<AppEvent, AppEffect> {
    match id {
        1 => {
            if !timer_1.is_active() {
                **active_timers += 1;
            }
            timer_1.start(AppEffect::SpawnTicker(1))
        }
        2 => {
            if !timer_2.is_active() {
                **active_timers += 1;
            }
            timer_2.start(AppEffect::SpawnTicker(2))
        }
        _ => Command::none(),
    }
}

/// ## Why use explicit cancel?
/// We stop the stream explicitly and drop its lease from model state.
fn stop_timer(
    id: u32,
    active_timers: &mut ActiveTimers,
    timer_1: &mut Timer1,
    timer_2: &mut Timer2,
) -> Command<AppEvent, AppEffect> {
    match id {
        1 => {
            if timer_1.is_active() {
                **active_timers -= 1;
            }
            timer_1.cancel()
        }
        2 => {
            if timer_2.is_active() {
                **active_timers -= 1;
            }
            timer_2.cancel()
        }
        _ => Command::none(),
    }
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
    match id {
        1 => {
            if timer_1.is_active() {
                **active_timers -= 1;
                timer_1.clear();
            }
        }
        2 => {
            if timer_2.is_active() {
                **active_timers -= 1;
                timer_2.clear();
            }
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
