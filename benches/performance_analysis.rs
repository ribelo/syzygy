//! Performance Benchmark for Syzygy Optimizations
//! 
//! This benchmark measures the impact of the implemented performance optimizations:
//! 1. Pre-allocated command buffer (eliminates Vec reallocations)
//! 2. Smart event loop optimization (conditional polling)
//! 3. O(1) task counting (atomic counter vs O(n) HashMap scan)
//! 4. Pre-sized collections (VecDeque and Vec with capacity)
//!
//! All optimizations use safe Rust and follow zero-copy patterns where possible.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::collections::HashMap;
use syzygy::prelude::*;

// ============================================================================
// Performance Test Application
// ============================================================================

#[derive(Debug, Clone)]
enum PerfEvent {
    ProcessBatch { count: u32 },
    BatchProcessed { processed: u32 },
    SpawnTask { id: u32 },
    TaskCompleted { id: u32 },
    QueryMetrics,
    MetricsReported { active_tasks: usize },
}

#[derive(Debug, Clone)]  
enum PerfEffect {
    ExecuteTask { id: u32 },
    LogMetrics { active_count: usize },
}

#[derive(Debug)]
#[derive(Default)]
struct PerfModel {
    processed_events: u64,
    batch_count: u32,
    tasks_spawned: u32,
    metrics_queries: u32,
}


#[derive(Default)]
struct PerfApp;

impl App for PerfApp {
    type Event = PerfEvent;
    type Model = PerfModel; 
    type Effect = PerfEffect;
    type Resources = ();

    fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
        model.processed_events += 1;

        match event {
            PerfEvent::ProcessBatch { count } => {
                model.batch_count += 1;
                Command::event(PerfEvent::BatchProcessed { processed: count })
            }
            
            PerfEvent::BatchProcessed { processed } => {
                // Simulate some work
                black_box(processed * 42);
                Command::none()
            }
            
            PerfEvent::SpawnTask { id } => {
                model.tasks_spawned += 1;
                Command::batch([
                    Command::event(PerfEvent::TaskCompleted { id }),
                    Command::effect(PerfEffect::ExecuteTask { id }),
                ])
            }
            
            PerfEvent::TaskCompleted { id: _ } => {
                Command::none()
            }
            
            PerfEvent::QueryMetrics => {
                model.metrics_queries += 1;
                Command::event(PerfEvent::MetricsReported { active_tasks: 5 })
            }
            
            PerfEvent::MetricsReported { active_tasks } => {
                Command::effect(PerfEffect::LogMetrics { active_count: active_tasks })
            }
        }
    }
}

// ============================================================================  
// Benchmark Functions
// ============================================================================

/// Benchmark event processing throughput with pre-allocated buffers
fn benchmark_event_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_processing");
    
    for batch_size in &[100, 1000, 5000] {
        group.bench_with_input(
            BenchmarkId::new("batch_processing", batch_size),
            batch_size,
            |b, &size| {
                b.iter(|| {
                    let (mut core, _shell) = Syzygy::builder::<PerfApp>()
                        .app(PerfApp)
                        .model(PerfModel::default())
                        .build();
                    
                    // Generate events that create command batches
                    for i in 0..size {
                        core.queue_event(PerfEvent::ProcessBatch { count: i });
                    }
                    
                    // This exercises the pre-allocated command buffer optimization
                    let commands = black_box(core.process_queued_events());
                    black_box(commands.len());
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark the smart event loop pattern
fn benchmark_event_loop_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_loop_efficiency");
    
    group.bench_function("mixed_workload", |b| {
        b.iter(|| {
            let (core, shell) = Syzygy::builder::<PerfApp>()
                .app(PerfApp)
                .model(PerfModel::default())
                .build();
            
            let mut runner = Runner::new(core, shell);
            let event_sender = runner.core().event_sender();
            
            // Create mixed workload: some queued events, some external
            for i in 0..50 {
                runner.core_mut().queue_event(PerfEvent::ProcessBatch { count: i });
                let _ = event_sender.send(PerfEvent::SpawnTask { id: i });
            }
            
            // This exercises the conditional polling optimization
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            
            rt.block_on(async {
                for _ in 0..10 {
                    let did_work = black_box(runner.tick(syzygy::spawn::TokioSpawn).await.unwrap());
                    black_box(did_work);
                }
            });
        });
    });
    
    group.finish();
}

/// Benchmark memory allocation patterns
fn benchmark_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");
    
    group.bench_function("repeated_allocations", |b| {
        b.iter(|| {
            let (mut core, _shell) = Syzygy::builder::<PerfApp>()
                .app(PerfApp)
                .model(PerfModel::default())
                .build();
            
            // This tests the pre-sized collections and command buffer reuse
            for batch in 0..100 {
                // Fill queue (pre-sized VecDeque)
                for i in 0..20 {
                    core.queue_event(PerfEvent::ProcessBatch { count: batch * 20 + i });
                }
                
                // Process (reuses command buffer)
                let commands = black_box(core.process_queued_events());
                black_box(commands.len());
            }
        });
    });
    
    group.finish();
}

/// Simulate the task counting optimization scenario  
fn benchmark_task_counting(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_counting");
    
    // Simulate the old O(n) approach for comparison
    group.bench_function("atomic_counter_simulation", |b| {
        use std::sync::atomic::{AtomicUsize, Ordering};
        
        b.iter(|| {
            let counter = AtomicUsize::new(0);
            
            // Simulate spawning/completing tasks  
            for _i in 0..1000 {
                counter.fetch_add(1, Ordering::Relaxed);
                let _count = black_box(counter.load(Ordering::Relaxed)); // O(1) 
                counter.fetch_sub(1, Ordering::Relaxed);
            }
        });
    });
    
    group.bench_function("hashmap_scan_simulation", |b| {
        b.iter(|| {
            let mut tasks: HashMap<u32, bool> = HashMap::new();
            
            // Simulate the old O(n) scanning approach
            for i in 0..1000 {
                tasks.insert(i, true);
                let _count = black_box(tasks.values().filter(|&&active| active).count()); // O(n)
                tasks.remove(&i);
            }
        });
    });
    
    group.finish();
}

/// Test the end-to-end system performance
fn benchmark_system_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("system_throughput");
    
    group.bench_function("full_system", |b| {
        b.iter(|| {
            let (core, shell) = Syzygy::builder::<PerfApp>()
                .app(PerfApp)
                .model(PerfModel::default())
                .build();
            
            let mut runner = Runner::new(core, shell);
            let event_sender = runner.core().event_sender();
            
            // Create realistic mixed workload
            for i in 0..200 {
                runner.core_mut().queue_event(PerfEvent::ProcessBatch { count: i });
                let _ = event_sender.send(PerfEvent::SpawnTask { id: i });
                if i.is_multiple_of(10) {
                    let _ = event_sender.send(PerfEvent::QueryMetrics);
                }
            }
            
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            
            rt.block_on(async {
                // Process until stable - tests all optimizations together
                runner.run_until(
                    |core, _shell| !core.has_queued_events(),
                    syzygy::spawn::TokioSpawn
                ).await.unwrap();
                
                let final_model = runner.core().model();
                black_box(final_model.processed_events);
                black_box(final_model.batch_count);
                black_box(final_model.tasks_spawned);
            });
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_event_processing,
    benchmark_event_loop_efficiency, 
    benchmark_memory_efficiency,
    benchmark_task_counting,
    benchmark_system_throughput
);
criterion_main!(benches);
