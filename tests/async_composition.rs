#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args)]
//! Comprehensive tests for async composition patterns
//!
//! This test suite verifies that the sequential, parallel, and race patterns
//! work correctly with actual effect execution and coordination semantics.

use std::time::{Duration, Instant};
use syzygy::event_context::EventContext;
use syzygy::prelude::*;
use tokio::time::sleep;

#[derive(Debug, Clone)]
#[allow(dead_code)] // Test code - variants used in async workflow testing
enum CompositionEvent {
    StartWorkflow,
    StepCompleted(String),
    WorkflowCompleted,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Test code - variants used for logging effect testing
#[allow(clippy::enum_variant_names)] // Test code - "Log" suffix is intentional for testing
enum CompositionEffect {
    DelayedLog { message: String, delay_ms: u64 },
    FastLog { message: String },
    SlowLog { message: String },
}

#[derive(Default)]
struct CompositionModel {
    events: Vec<String>,
    completed_steps: Vec<String>,
}

use syzygy::storage::{EmptyStorage, Storage};

fn composition_update(
    event: CompositionEvent,
    ctx: &mut EventContext<
        CompositionEvent,
        CompositionEffect,
        Storage<CompositionModel, EmptyStorage>,
    >,
) -> Command<CompositionEvent, CompositionEffect> {
    let model: &mut CompositionModel = ctx.model_mut();
    match event {
        CompositionEvent::StartWorkflow => {
            model.events.push("Workflow started".to_string());

            // Test sequential effects - should execute in order
            Command::sequence([
                Command::effect(CompositionEffect::DelayedLog {
                    message: "Step 1".to_string(),
                    delay_ms: 10,
                }),
                Command::effect(CompositionEffect::DelayedLog {
                    message: "Step 2".to_string(),
                    delay_ms: 10,
                }),
                Command::effect(CompositionEffect::DelayedLog {
                    message: "Step 3".to_string(),
                    delay_ms: 10,
                }),
            ])
        }
        CompositionEvent::StepCompleted(step) => {
            model.completed_steps.push(step);
            Command::none()
        }
        CompositionEvent::WorkflowCompleted => {
            model.events.push("Workflow completed".to_string());
            Command::none()
        }
    }
}

/// Test effect handler - no state capture, just executes effects
async fn create_effect_handler(
    effect: CompositionEffect,
    ctx: EffectContext<CompositionEvent, EmptyStorage>,
) {
    match effect {
        CompositionEffect::DelayedLog { message, delay_ms } => {
            sleep(Duration::from_millis(delay_ms)).await;
            // Send completion event instead of logging to captured state
            let _ = ctx.send_event(CompositionEvent::StepCompleted(message));
        }
        CompositionEffect::FastLog { message } => {
            sleep(Duration::from_millis(5)).await;
            let _ = ctx.send_event(CompositionEvent::StepCompleted(message));
        }
        CompositionEffect::SlowLog { message } => {
            sleep(Duration::from_millis(50)).await;
            let _ = ctx.send_event(CompositionEvent::StepCompleted(message));
        }
    }
}

#[tokio::test]
async fn test_sequential_effects_execution_order() {
    // Build the system - no state capture needed!
    let (core, shell) = Syzygy::builder::<CompositionEvent, CompositionEffect>()
        .model(CompositionModel::default())
        .event_handler(composition_update)
        .effect_handler(create_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);

    // Start the workflow
    runner
        .core()
        .send_event(CompositionEvent::StartWorkflow)
        .unwrap();

    // Process effects until workflow completes
    let start_time = Instant::now();
    let timeout = Duration::from_millis(200);

    loop {
        let did_work = runner.tick(syzygy::spawn::spawner()).await.unwrap();
        if !did_work {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // Check if all steps completed
        if runner.core().model().completed_steps.len() >= 3 {
            break;
        }

        assert!(
            start_time.elapsed() <= timeout,
            "Test timeout - only {} steps completed",
            runner.core().model().completed_steps.len()
        );
    }

    let total_time = start_time.elapsed();

    // Verify execution order using model data (no captured state!)
    let model = runner.core().model();
    assert_eq!(model.completed_steps.len(), 3, "Expected 3 completed steps");
    assert_eq!(model.completed_steps[0], "Step 1");
    assert_eq!(model.completed_steps[1], "Step 2");
    assert_eq!(model.completed_steps[2], "Step 3");

    // Verify timing - sequential should take at least 30ms (3 * 10ms)
    assert!(
        total_time >= Duration::from_millis(25),
        "Sequential execution too fast: {total_time:?}"
    );
}

#[tokio::test]
async fn test_mixed_coordination_patterns() {
    let (core, shell) = Syzygy::builder::<CompositionEvent, CompositionEffect>()
        .model(CompositionModel::default())
        .event_handler(composition_update)
        .effect_handler(create_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);

    // Test a combination of patterns through direct shell execution
    let mixed_command = Command::batch([
        // First do something sequential
        Command::sequence([
            Command::effect(CompositionEffect::FastLog {
                message: "Seq1".to_string(),
            }),
            Command::effect(CompositionEffect::FastLog {
                message: "Seq2".to_string(),
            }),
        ]),
        // Then some individual effects (default parallel execution by Shell)
        Command::parallel([
            CompositionEffect::DelayedLog {
                message: "Par1".to_string(),
                delay_ms: 10,
            },
            CompositionEffect::DelayedLog {
                message: "Par2".to_string(),
                delay_ms: 10,
            },
        ]),
    ]);

    runner.shell_mut().dispatch(mixed_command).unwrap();

    // Process until all effects complete
    let timeout = Duration::from_millis(500);
    let start_time = Instant::now();

    loop {
        let did_work = runner.tick(syzygy::spawn::spawner()).await.unwrap();
        if !did_work {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }

        // Check if all expected steps completed
        if runner.core().model().completed_steps.len() >= 4 {
            break;
        }

        assert!(
            start_time.elapsed() <= timeout,
            "Test timeout - only {} steps completed",
            runner.core().model().completed_steps.len()
        );
    }

    // Verify all effects executed using model data (no state capture!)
    let model = runner.core().model();
    assert_eq!(model.completed_steps.len(), 4, "Expected 4 completed steps");

    // Extract messages to check they're all there
    let messages: std::collections::HashSet<_> = model.completed_steps.iter().cloned().collect();
    assert!(messages.contains("Seq1"));
    assert!(messages.contains("Seq2"));
    assert!(messages.contains("Par1"));
    assert!(messages.contains("Par2"));
}
