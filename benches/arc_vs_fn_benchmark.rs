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
//! Arc vs Function Pointer Benchmark for Effect Handler Performance
//!
//! This benchmark measures the specific performance impact of using Arc<dyn Fn>
//! vs function pointers for effect handlers in Syzygy's Shell implementation.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::future::BoxFuture;
use std::sync::Arc;

// Simulated effect and event types for benchmarking
#[derive(Clone)]
struct BenchEffect {
    id: u32,
    data: String,
}

#[derive(Clone)]
#[allow(dead_code)]
struct BenchEvent {
    result: String,
}

// Simulated AsyncContext (minimal for benchmarking)
#[derive(Copy, Clone)]
struct MockAsyncContext;

impl MockAsyncContext {
    fn send_event(self, _event: BenchEvent) -> Result<(), ()> {
        Ok(())
    }
}

// Arc-based handler (current implementation)
type ArcHandler =
    Arc<dyn Fn(BenchEffect, MockAsyncContext) -> BoxFuture<'static, ()> + Send + Sync>;

// Function pointer handler (proposed optimization)
type FnHandler = fn(BenchEffect, MockAsyncContext) -> BoxFuture<'static, ()>;

// Sample effect handler implementation
fn sample_effect_handler(effect: BenchEffect, ctx: MockAsyncContext) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        // Simulate some work
        let _result = format!("Processed effect {}: {}", effect.id, effect.data);
        let _ = ctx.send_event(BenchEvent {
            result: "completed".to_string(),
        });
    })
}

fn benchmark_arc_cloning(c: &mut Criterion) {
    let handler: ArcHandler = Arc::new(sample_effect_handler);

    c.bench_function("arc_clone_only", |b| {
        b.iter(|| {
            let cloned = Arc::clone(&handler);
            black_box(cloned);
        });
    });
}

fn benchmark_effect_execution(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);
    let fn_handler: FnHandler = sample_effect_handler;

    let effect = BenchEffect {
        id: 42,
        data: "test_data".to_string(),
    };
    let ctx = MockAsyncContext;

    let mut group = c.benchmark_group("effect_execution");

    // Benchmark Arc-based handler (with clone overhead)
    group.bench_function("arc_handler", |b| {
        b.iter(|| {
            let cloned_handler = Arc::clone(&arc_handler);
            let effect_clone = effect.clone();
            let future = cloned_handler(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    // Benchmark function pointer (zero overhead)
    group.bench_function("fn_handler", |b| {
        b.iter(|| {
            let effect_clone = effect.clone();
            let future = fn_handler(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    group.finish();
}

fn benchmark_batch_effects(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);
    let fn_handler: FnHandler = sample_effect_handler;

    let effects: Vec<BenchEffect> = (0..100)
        .map(|i| BenchEffect {
            id: i,
            data: format!("batch_data_{i}"),
        })
        .collect();

    let ctx = MockAsyncContext;

    let mut group = c.benchmark_group("batch_effects");

    // Batch processing with Arc (current implementation)
    group.bench_function("arc_batch", |b| {
        b.iter(|| {
            for effect in &effects {
                let cloned_handler = Arc::clone(&arc_handler);
                let effect_clone = effect.clone();
                let future = cloned_handler(effect_clone, ctx);
                #[allow(unused_must_use)]
                black_box(future);
            }
        });
    });

    // Batch processing with function pointer
    group.bench_function("fn_batch", |b| {
        b.iter(|| {
            for effect in &effects {
                let effect_clone = effect.clone();
                let future = fn_handler(effect_clone, ctx);
                #[allow(unused_must_use)]
                black_box(future);
            }
        });
    });

    group.finish();
}

fn benchmark_memory_access_patterns(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);
    let fn_handler: FnHandler = sample_effect_handler;

    let effect = BenchEffect {
        id: 1,
        data: "memory_test".to_string(),
    };
    let ctx = MockAsyncContext;

    let mut group = c.benchmark_group("memory_patterns");

    // Measure the cost of Arc indirection
    group.bench_function("arc_indirection", |b| {
        b.iter(|| {
            // This simulates the typical usage pattern in Shell::tick()
            // where we clone the Arc for each effect execution
            let cloned = Arc::clone(&arc_handler);
            let effect_clone = effect.clone();

            // The actual function call through Arc indirection
            let future = cloned(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    // Direct function call (zero indirection)
    group.bench_function("fn_direct", |b| {
        b.iter(|| {
            let effect_clone = effect.clone();
            let future = fn_handler(effect_clone, ctx);
            #[allow(unused_must_use)]
            black_box(future);
        });
    });

    group.finish();
}

fn benchmark_concurrent_access(c: &mut Criterion) {
    let arc_handler: ArcHandler = Arc::new(sample_effect_handler);

    let mut group = c.benchmark_group("concurrent_access");

    // Simulate concurrent Arc cloning (potential cache line contention)
    group.bench_function("arc_concurrent_clone", |b| {
        b.iter(|| {
            // Simulate multiple concurrent clones
            let handlers: Vec<_> = (0..4).map(|_| Arc::clone(&arc_handler)).collect();
            black_box(handlers);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_arc_cloning,
    benchmark_effect_execution,
    benchmark_batch_effects,
    benchmark_memory_access_patterns,
    benchmark_concurrent_access
);
criterion_main!(benches);
