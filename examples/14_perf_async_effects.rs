use std::hint::black_box;
use std::time::{Duration, Instant};

use syzygy::prelude::*;

const SAMPLES: usize = 20;
const SAMPLES_F64: f64 = 20.0;
const BATCH_SIZE: usize = 10_000;
const BATCH_SIZE_F64: f64 = 10_000.0;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Completed)]
    completed: usize,
}

#[derive(Debug, Clone, Copy)]
enum Event {
    TriggerImmediate,
    TriggerOnce,
    Completed,
}

#[derive(Debug, Clone, Copy)]
enum Effect {
    ImmediateDone,
    OnceDone,
}

fn completed(completed: &mut Completed) -> Command<Event, Effect> {
    *completed.get_mut() += 1;
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
    match event {
        Event::TriggerImmediate => Command::effect(Effect::ImmediateDone),
        Event::TriggerOnce => Command::effect(Effect::OnceDone),
        Event::Completed => handle!(completed, ctx),
    }
}

fn handle_effect(effect: Effect, _ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    match effect {
        Effect::ImmediateDone => Task::resolved(Command::event(Event::Completed)),
        Effect::OnceDone => Task::once(async { Command::event(Event::Completed) }),
    }
}

fn app() -> Result<Syzygy<Event, Effect, AppModel>, ShellError> {
    Syzygy::builder::<Event, Effect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .with_event_channel_capacity(None)
        .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::ZERO))
        .build()
}

fn run_batch(app: &mut Syzygy<Event, Effect, AppModel>, event: Event) -> Result<(), ShellError> {
    let target = app.model().completed + BATCH_SIZE;
    for _ in 0..BATCH_SIZE {
        app.core().emit(event);
    }
    app.run_until(|core, _| core.model().completed >= target)?;
    black_box(app.model().completed);
    Ok(())
}

fn measure(name: &str, event: Event) -> Result<(), ShellError> {
    let mut app = app()?;
    run_batch(&mut app, event)?;
    let mut samples = Vec::with_capacity(SAMPLES);

    for _ in 0..SAMPLES {
        let start = Instant::now();
        run_batch(&mut app, event)?;
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0 / BATCH_SIZE_F64);
    }

    samples.sort_by(f64::total_cmp);
    let sum = samples.iter().sum::<f64>();
    let mean = sum / SAMPLES_F64;
    let median = samples[samples.len() / 2];
    let min = samples[0];
    let max = samples[samples.len() - 1];

    println!(
        "{{\"name\":\"{name}\",\"runs\":{SAMPLES},\"batch_size\":{BATCH_SIZE},\"min\":{min},\"max\":{max},\"mean\":{mean},\"median\":{median}}}"
    );
    println!("{name}_model={}", app.model().completed);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    measure("syzygy_immediate_effect_batch", Event::TriggerImmediate)?;
    measure("syzygy_async_once_effect_batch", Event::TriggerOnce)?;
    Ok(())
}
