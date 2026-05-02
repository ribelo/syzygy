use std::hint::black_box;
use std::time::Instant;

use syzygy::prelude::*;

const SAMPLES: usize = 20;
const SAMPLES_F64: f64 = 20.0;
const BATCH_SIZE: usize = 100_000;
const BATCH_SIZE_F64: f64 = 100_000.0;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Counter)]
    counter: usize,
}

#[derive(Debug, Clone, Copy)]
enum Event {
    Increment,
}

#[derive(Debug, Clone, Copy)]
enum Effect {}

fn increment(counter: &mut Counter) -> Command<Event, Effect> {
    *counter.get_mut() += 1;
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
    match event {
        Event::Increment => handle!(increment, ctx),
    }
}

fn main() {
    let (mut core, _sender) =
        Core::with_event_channel_capacity(Box::new(handle_event), AppModel::default(), None);
    let mut samples = Vec::with_capacity(SAMPLES);

    for _ in 0..SAMPLES {
        let start = Instant::now();
        for _ in 0..BATCH_SIZE {
            let command = core.handle_event(Event::Increment);
            black_box(command);
        }
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0 / BATCH_SIZE_F64);
    }

    samples.sort_by(f64::total_cmp);
    let sum = samples.iter().sum::<f64>();
    let mean = sum / SAMPLES_F64;
    let median = samples[samples.len() / 2];
    let min = samples[0];
    let max = samples[samples.len() - 1];

    println!(
        "{{\"name\":\"syzygy_core_increment\",\"runs\":{SAMPLES},\"batch_size\":{BATCH_SIZE},\"min\":{min},\"max\":{max},\"mean\":{mean},\"median\":{median}}}"
    );
    println!("model={}", core.model().counter);
}
