#![allow(
    dead_code,
    clippy::clone_on_ref_ptr,
    unused_variables,
    unused_imports,
    clippy::let_and_return,
    clippy::format_in_format_args,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::derivable_impls,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::type_complexity,
    clippy::duplicated_attributes
)]
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use syzygy::command::CommandStep;
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum BenchEvent {
    A,
    B,
    C,
    D,
    E,
}

#[derive(Debug, Clone)]
enum BenchEffect {
    X,
    Y,
    Z,
    W,
    V,
}

fn bench_command_creation(c: &mut Criterion) {
    c.bench_function("command_none", |b| {
        b.iter(|| black_box(Command::<BenchEvent, BenchEffect>::none()));
    });

    c.bench_function("command_single_event", |b| {
        b.iter(|| black_box(Command::<BenchEvent, BenchEffect>::event(BenchEvent::A)));
    });

    c.bench_function("command_single_effect", |b| {
        b.iter(|| black_box(Command::<BenchEvent, BenchEffect>::effect(BenchEffect::X)));
    });

    // Test the SmallVec inline optimization (≤4 items)
    c.bench_function("command_batch_4_items", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch([
                Command::event(BenchEvent::A),
                Command::event(BenchEvent::B),
                Command::effect(BenchEffect::X),
                Command::effect(BenchEffect::Y),
            ]))
        });
    });

    // Test SmallVec spill case (>4 items)
    c.bench_function("command_batch_8_items", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch([
                Command::event(BenchEvent::A),
                Command::event(BenchEvent::B),
                Command::event(BenchEvent::C),
                Command::event(BenchEvent::D),
                Command::effect(BenchEffect::X),
                Command::effect(BenchEffect::Y),
                Command::effect(BenchEffect::Z),
                Command::effect(BenchEffect::W),
            ]))
        });
    });
}

fn bench_command_iteration(c: &mut Criterion) {
    let small_cmd = Command::<BenchEvent, BenchEffect>::batch([
        Command::event(BenchEvent::A),
        Command::event(BenchEvent::B),
        Command::effect(BenchEffect::X),
        Command::effect(BenchEffect::Y),
    ]);

    let large_cmd = Command::<BenchEvent, BenchEffect>::batch([
        Command::event(BenchEvent::A),
        Command::event(BenchEvent::B),
        Command::event(BenchEvent::C),
        Command::event(BenchEvent::D),
        Command::event(BenchEvent::E),
        Command::effect(BenchEffect::X),
        Command::effect(BenchEffect::Y),
        Command::effect(BenchEffect::Z),
        Command::effect(BenchEffect::W),
        Command::effect(BenchEffect::V),
    ]);

    c.bench_function("iterate_small_command", |b| {
        b.iter(|| {
            let mut count = 0;
            for output in black_box(small_cmd.clone()) {
                count += match output {
                    CommandStep::Event(_) | CommandStep::Effect(_) => 1,
                    CommandStep::Batch(e) | CommandStep::Parallel(e) => e.len(),
                };
            }
            black_box(count)
        });
    });

    c.bench_function("iterate_large_command", |b| {
        b.iter(|| {
            let mut count = 0;
            for output in black_box(large_cmd.clone()) {
                count += match output {
                    CommandStep::Event(_) | CommandStep::Effect(_) => 1,
                    CommandStep::Batch(e) | CommandStep::Parallel(e) => e.len(),
                };
            }
            black_box(count)
        });
    });
}

fn bench_command_composition(c: &mut Criterion) {
    let base_commands: Vec<_> = (0..100)
        .map(|i| {
            if i % 2 == 0 {
                Command::<BenchEvent, BenchEffect>::event(BenchEvent::A)
            } else {
                Command::<BenchEvent, BenchEffect>::effect(BenchEffect::X)
            }
        })
        .collect();

    c.bench_function("batch_100_commands", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch(
                base_commands.clone(),
            ))
        });
    });

    c.bench_function("batch_commands", |b| {
        b.iter(|| {
            black_box(Command::<BenchEvent, BenchEffect>::batch(
                base_commands.clone(),
            ))
        });
    });
}

fn bench_memory_efficiency(c: &mut Criterion) {
    use std::mem;

    // Verify SmallVec doesn't allocate for small commands
    c.bench_function("verify_inline_storage", |b| {
        b.iter(|| {
            let cmd = Command::<BenchEvent, BenchEffect>::batch([
                Command::event(BenchEvent::A),
                Command::event(BenchEvent::B),
                Command::effect(BenchEffect::X),
                Command::effect(BenchEffect::Y),
            ]);
            // This should be inline (no heap allocation)
            black_box(mem::size_of_val(&cmd))
        });
    });
}

criterion_group!(
    benches,
    bench_command_creation,
    bench_command_iteration,
    bench_command_composition,
    bench_memory_efficiency
);
criterion_main!(benches);
