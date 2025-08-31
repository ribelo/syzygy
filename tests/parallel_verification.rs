#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args)]
//! Test to verify that parallel effects truly execute in parallel
//!
//! This test verifies that the Group coordination pattern
//! correctly runs effects concurrently, not sequentially.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use syzygy::event_context::EventContext;
use syzygy::prelude::*;
use syzygy::command::Effects;
use syzygy::streaming::EffectOutput;
use tokio::time::sleep;

#[derive(Debug, Clone)]
enum ParallelEvent {
    StartParallel,
}

#[derive(Debug, Clone)]
enum ParallelEffect {
    Step { id: u32, duration_ms: u64 },
}

#[derive(Default)]
struct ParallelModel;

/// Test resources for capturing execution timing
#[derive(Clone)]
struct ParallelResources {
    execution_log: Arc<Mutex<Vec<(u32, Instant, Instant)>>>,
}

use syzygy::storage::{EmptyStorage, Storage};

fn parallel_update(
    event: ParallelEvent,
    _ctx: &mut EventContext<ParallelEvent, ParallelEffect, Storage<ParallelModel, EmptyStorage>>,
) -> Command<ParallelEvent, ParallelEffect> {
    match event {
        ParallelEvent::StartParallel => {
            // These should execute in parallel: all starting at the same time
            // Each step takes 100ms, so total should be ~100ms for parallel execution
            Effects::new([
                ParallelEffect::Step {
                    id: 1,
                    duration_ms: 100,
                },
                ParallelEffect::Step {
                    id: 2,
                    duration_ms: 100,
                },
                ParallelEffect::Step {
                    id: 3,
                    duration_ms: 100,
                },
            ]).parallel().spawn()
        }
    }
}

/// Effect handler that records when each effect starts and completes
/// Uses Resources for clean dependency injection - no state capture!
async fn parallel_effect_handler(
    effect: ParallelEffect,
    ctx: EffectContext<ParallelEvent, Storage<ParallelResources, EmptyStorage>>,
) -> EffectOutput<ParallelEvent> {
    let start_time = Instant::now();
    let resources: &ParallelResources = ctx.resource();

    match effect {
        ParallelEffect::Step { id, duration_ms } => {
            sleep(Duration::from_millis(duration_ms)).await;
            let end_time = Instant::now();
            resources
                .execution_log
                .lock()
                .unwrap()
                .push((id, start_time, end_time));
        }
    }
    EffectOutput::None
}

#[tokio::test]
async fn test_parallel_effects_are_actually_parallel() {
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let resources = ParallelResources {
        execution_log: execution_log.clone(),
    };

    let (core, shell) = Syzygy::builder::<ParallelEvent, ParallelEffect>()
        .model(ParallelModel)
        .resource(resources)
        .event_handler(parallel_update)
        .effect_handler(parallel_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);

    // Record overall start time
    let overall_start = Instant::now();

    // Start the parallel execution
    runner
        .core()
        .send_event(ParallelEvent::StartParallel)
        .unwrap();

    // Process effects and wait for completion
    for _ in 0..30 {
        // Give time for effects to complete
        runner.tick(syzygy::spawn::spawner()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let overall_duration = overall_start.elapsed();
    let log = execution_log.lock().unwrap();

    // Verify all effects completed
    assert_eq!(log.len(), 3, "Expected 3 effects to complete");

    // Sort by id to ensure consistent ordering
    let mut sorted_log = log.clone();
    sorted_log.sort_by_key(|(id, _, _)| *id);

    println!("Parallel execution log:");
    for (id, start, end) in &sorted_log {
        println!(
            "  Step {}: {:?} -> {:?} (duration: {:?})",
            id,
            start.duration_since(overall_start),
            end.duration_since(overall_start),
            end.duration_since(*start)
        );
    }

    println!("Overall duration: {overall_duration:?}");

    // KEY TEST: If truly parallel, all effects should start at approximately the same time
    let start_times: Vec<_> = sorted_log.iter().map(|(_, start, _)| *start).collect();
    let first_start = start_times[0];

    // All effects should start within a small window (< 50ms)
    for (i, start_time) in start_times.iter().enumerate() {
        let start_diff = start_time.duration_since(first_start);
        println!(
            "  Step {} started {} after Step 1",
            i + 1,
            format!("{:?}", start_diff)
        );

        assert!(
            start_diff < Duration::from_millis(50),
            "Step {} started too late ({:?}), indicates sequential not parallel execution",
            i + 1,
            start_diff
        );
    }

    // The key test is that all effects started together, not the total test time
    // (total test time includes our polling loop which adds overhead)

    // Real test: look at when effects actually completed relative to when they started
    let actual_execution_time = sorted_log
        .iter()
        .map(|(_, start, end)| end.duration_since(*start))
        .max()
        .unwrap_or(Duration::ZERO);

    println!("  Actual effect execution time: {actual_execution_time:?}");

    // Parallel execution should complete in ~100ms per effect, not 300ms total
    assert!(
        actual_execution_time <= Duration::from_millis(120),
        "Individual effect took too long ({actual_execution_time:?}), timing issue"
    );

    println!("✅ All effects started within 50ms window - TRUE PARALLEL EXECUTION!");
}

#[tokio::test]
async fn test_mixed_sequential_and_parallel() {
    // Test combining sequential and parallel patterns
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let resources = ParallelResources {
        execution_log: execution_log.clone(),
    };

    let (core, shell) = Syzygy::builder::<ParallelEvent, ParallelEffect>()
        .model(ParallelModel)
        .resource(resources)
        .event_handler(parallel_update)
        .effect_handler(parallel_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);

    // Mix sequential and parallel patterns
    let mixed_command = Command::batch([
        // First: parallel execution (should complete in ~100ms)
        Effects::new([
            ParallelEffect::Step {
                id: 10,
                duration_ms: 80,
            },
            ParallelEffect::Step {
                id: 11,
                duration_ms: 80,
            },
        ]).parallel().spawn(),
        // Then: sequential execution (should complete in ~160ms total)
        Command::sequence([
            Command::effect(ParallelEffect::Step {
                id: 20,
                duration_ms: 80,
            }),
            Command::effect(ParallelEffect::Step {
                id: 21,
                duration_ms: 80,
            }),
        ]),
    ]);

    let overall_start = Instant::now();
    runner.shell_mut().dispatch(mixed_command).unwrap();

    for _ in 0..50 {
        runner.tick(syzygy::spawn::spawner()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let overall_duration = overall_start.elapsed();
    let log = execution_log.lock().unwrap();

    println!("Mixed execution results:");
    println!("  Total effects completed: {}", log.len());
    println!("  Overall duration: {overall_duration:?}");

    // Should have all 4 effects
    assert_eq!(log.len(), 4, "Expected 4 effects to complete");

    // Parallel effects (10, 11) should start at similar times
    let parallel_effects: Vec<_> = log
        .iter()
        .filter(|(id, _, _)| *id >= 10 && *id < 20)
        .collect();
    if parallel_effects.len() == 2 {
        let start_diff = parallel_effects[1].1.duration_since(parallel_effects[0].1);
        println!("  Parallel effects start difference: {start_diff:?}");
        assert!(
            start_diff < Duration::from_millis(30),
            "Parallel effects didn't start together"
        );
    }

    // Sequential effects (20, 21) should start one after the other
    let sequential_effects: Vec<_> = log.iter().filter(|(id, _, _)| *id >= 20).collect();
    if sequential_effects.len() == 2 {
        // Sort by start time to see which came first
        let mut seq_sorted = sequential_effects;
        seq_sorted.sort_by_key(|(_, start, _)| *start);

        let first_end = seq_sorted[0].2; // End time of first effect
        let second_start = seq_sorted[1].1; // Start time of second effect

        println!(
            "  Sequential: first ended at {:?}, second started at {:?}",
            first_end.duration_since(overall_start),
            second_start.duration_since(overall_start)
        );

        // Second should start after first ends (with small tolerance)
        let tolerance = Duration::from_millis(20);
        let earliest_acceptable_start = if first_end.elapsed() < tolerance {
            overall_start
        } else {
            first_end.checked_sub(tolerance).unwrap()
        };

        assert!(
            second_start >= earliest_acceptable_start,
            "Sequential effects didn't execute in order"
        );
    }

    println!("✅ Mixed sequential and parallel patterns work correctly!");
}
