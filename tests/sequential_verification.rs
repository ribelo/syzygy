//! Test to verify that sequential effects truly execute sequentially
//!
//! This test demonstrates the current bug: effects marked as sequential
//! are actually executing in parallel because each effect spawns its own task.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use syzygy::event_context::EventContext;
use syzygy::prelude::*;
use tokio::time::sleep;

#[derive(Debug, Clone)]
enum SequentialEvent {
    StartSequence,
}

#[derive(Debug, Clone)]
enum SequentialEffect {
    Step { id: u32, duration_ms: u64 },
}

#[derive(Default)]
struct SequentialModel;

/// Test resources for capturing execution timing
#[derive(Clone)]
struct SequentialResources {
    execution_log: Arc<Mutex<Vec<(u32, Instant, Instant)>>>,
}

use syzygy::storage::{EmptyStorage, Storage};

fn sequential_update(
    event: SequentialEvent,
    _ctx: &mut EventContext<
        SequentialEvent,
        SequentialEffect,
        Storage<SequentialModel, EmptyStorage>,
    >,
) -> Command<SequentialEvent, SequentialEffect> {
    match event {
        SequentialEvent::StartSequence => {
            // These should execute in order: Step 1, then Step 2, then Step 3
            // Each step takes 100ms, so total should be ~300ms for sequential execution
            Command::sequence([
                Command::effect(SequentialEffect::Step {
                    id: 1,
                    duration_ms: 100,
                }),
                Command::effect(SequentialEffect::Step {
                    id: 2,
                    duration_ms: 100,
                }),
                Command::effect(SequentialEffect::Step {
                    id: 3,
                    duration_ms: 100,
                }),
            ])
        }
    }
}

/// Effect handler that records when each effect starts and completes
/// Uses Resources for clean dependency injection - no state capture!
async fn sequential_effect_handler(
    effect: SequentialEffect,
    ctx: EffectContext<SequentialEvent, Storage<SequentialResources, EmptyStorage>>,
) {
    let start_time = Instant::now();
    let resources: &SequentialResources = ctx.resource();

    match effect {
        SequentialEffect::Step { id, duration_ms } => {
            sleep(Duration::from_millis(duration_ms)).await;
            let end_time = Instant::now();
            resources
                .execution_log
                .lock()
                .unwrap()
                .push((id, start_time, end_time));
        }
    }
}

#[tokio::test]
async fn test_sequential_effects_are_actually_sequential() {
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let resources = SequentialResources {
        execution_log: execution_log.clone(),
    };

    let (core, shell) = Syzygy::builder::<SequentialEvent, SequentialEffect>()
        .model(SequentialModel)
        .resource(resources)
        .update(sequential_update)
        .build();

    let shell = shell.with_effect_handler(sequential_effect_handler);
    let mut runner = Runner::new(core, shell);

    // Record overall start time
    let overall_start = Instant::now();

    // Start the sequence
    runner
        .core()
        .send_event(SequentialEvent::StartSequence)
        .unwrap();

    // Process effects and wait for completion
    for _ in 0..50 {
        // Give plenty of time for effects to complete
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

    println!("Execution log:");
    for (id, start, end) in &sorted_log {
        println!(
            "  Step {}: {:?} -> {:?} (duration: {:?})",
            id,
            start.duration_since(overall_start),
            end.duration_since(overall_start),
            end.duration_since(*start)
        );
    }

    // KEY TEST: If truly sequential, Step 2 should start AFTER Step 1 completes
    let step1_end = sorted_log.iter().find(|(id, _, _)| *id == 1).unwrap().2;
    let step2_start = sorted_log.iter().find(|(id, _, _)| *id == 2).unwrap().1;
    let step3_start = sorted_log.iter().find(|(id, _, _)| *id == 3).unwrap().1;

    println!("Sequential check:");
    println!(
        "  Step 1 ended at: {:?}",
        step1_end.duration_since(overall_start)
    );
    println!(
        "  Step 2 started at: {:?}",
        step2_start.duration_since(overall_start)
    );
    println!(
        "  Step 3 started at: {:?}",
        step3_start.duration_since(overall_start)
    );
    println!("  Overall duration: {overall_duration:?}");

    // BUG DEMONSTRATION: This assertion will FAIL because effects run in parallel
    // Step 2 starts immediately instead of waiting for Step 1 to complete
    assert!(
        step2_start >= step1_end,
        "BUG: Step 2 started before Step 1 completed! Effects are parallel, not sequential."
    );

    // Additional verification: total time should be ~300ms for sequential, ~100ms for parallel
    assert!(
        overall_duration >= Duration::from_millis(250),
        "BUG: Total execution too fast ({overall_duration:?}), indicates parallel execution"
    );
}

#[tokio::test]
async fn test_parallel_baseline_for_comparison() {
    // This test shows what parallel execution looks like for comparison
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let resources = SequentialResources {
        execution_log: execution_log.clone(),
    };

    let (core, shell) = Syzygy::builder::<SequentialEvent, SequentialEffect>()
        .model(SequentialModel)
        .resource(resources)
        .update(sequential_update)
        .build();

    let shell = shell.with_effect_handler(sequential_effect_handler);
    let mut runner = Runner::new(core, shell);

    let overall_start = Instant::now();

    // Use individual effects (which spawn separate tasks = parallel execution)
    let parallel_command = Command::parallel([
        SequentialEffect::Step {
            id: 1,
            duration_ms: 100,
        },
        SequentialEffect::Step {
            id: 2,
            duration_ms: 100,
        },
        SequentialEffect::Step {
            id: 3,
            duration_ms: 100,
        },
    ]);

    runner.shell_mut().dispatch(parallel_command).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        runner.tick(syzygy::spawn::spawner()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        if execution_log.lock().unwrap().len() >= 3 {
            break;
        }
        if std::time::Instant::now() > deadline {
            break;
        }
    }

    let overall_duration = overall_start.elapsed();
    let log = execution_log.lock().unwrap();

    assert_eq!(log.len(), 3, "Expected 3 effects to complete");

    println!("Parallel execution duration: {overall_duration:?}");

    // Parallel execution should be ~100ms (all effects run simultaneously)
    assert!(
        overall_duration < Duration::from_millis(200),
        "Parallel execution should be fast (~100ms), got {overall_duration:?}"
    );
}
