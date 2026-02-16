use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::time::Duration;
use syzygy::command::Command;
use syzygy::executor::Task;
use syzygy::prelude::Syzygy;
use syzygy::syzygy::SyzygyConfig;

#[derive(Default)]
struct BenchModel;

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum BenchEvent {
    Tick,
}

#[derive(Clone, Copy)]
enum BenchEffect {
    Nop,
}

fn update(_event: BenchEvent, _model: &mut BenchModel) -> Command<BenchEvent, BenchEffect> {
    Command::effect(BenchEffect::Nop)
}

fn effects(effect: BenchEffect, _resources: ()) -> Task<BenchEvent, BenchEffect> {
    match effect {
        BenchEffect::Nop => Task::none(),
    }
}

fn command_routing_bench(c: &mut Criterion) {
    const EFFECTS_PER_ITER: usize = 128;

    let mut group = c.benchmark_group("shell");
    group.throughput(Throughput::Elements(EFFECTS_PER_ITER as u64));
    group.bench_function("dispatch_command_128_effects", |b| {
        let mut runner = Syzygy::builder::<BenchEvent, BenchEffect>()
            .model(BenchModel::default())
            .event_handler(update)
            .effect_handler(effects)
            .with_effect_channel_capacity(Some(256))
            .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
            .build();

        b.iter(|| {
            {
                let shell = runner.shell_mut();
                for _ in 0..EFFECTS_PER_ITER {
                    shell
                        .dispatch_command(Command::effect(BenchEffect::Nop))
                        .expect("command dispatch succeeds");
                }
            }
            runner.shell_mut().drain().expect("drain succeeds");
        });
    });

    group.bench_function("dispatch_and_drain_256_effects", |b| {
        let mut runner = Syzygy::builder::<BenchEvent, BenchEffect>()
            .model(BenchModel::default())
            .event_handler(update)
            .effect_handler(effects)
            .with_effect_channel_capacity(Some(256))
            .with_syzygy_config(SyzygyConfig::default().idle_sleep(Duration::from_millis(1)))
            .build();

        b.iter(|| {
            {
                let shell = runner.shell_mut();
                for _ in 0..256 {
                    shell
                        .dispatch_command(Command::effect(BenchEffect::Nop))
                        .expect("command dispatch succeeds");
                }
            }
            runner.shell_mut().drain().expect("drain succeeds");
        });
    });

    group.finish();
}

criterion_group!(shell_benches, command_routing_bench);
criterion_main!(shell_benches);
