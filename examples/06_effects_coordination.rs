#![allow(clippy::uninlined_format_args)] // Educational examples use explicit format for clarity

//! Example 06: Effect Coordination with the New Effects API
//!
//! This example demonstrates the new iterator-style Effects API for coordinating
//! async effects. You'll learn:
//! - Parallel coordination with barriers
//! - Race coordination (first-wins)
//! - Sequential workflows
//! - Timeout handling per effect
//! - Real-world coordination patterns

use std::time::Duration;
use syzygy::prelude::*;
use syzygy::streaming::EffectResult;

// ============================================================================
// Application State & Events
// ============================================================================

#[derive(Debug, Clone, Default)]
struct AppModel {
    bootstrap_complete: bool,
    config: Option<String>,
    user_data: Option<String>,
    fastest_mirror: Option<String>,
    workflow_step: u32,
    messages: Vec<String>,
}

#[derive(Debug, Clone)]
enum AppEvent {
    // Bootstrap events (parallel coordination)
    StartBootstrap,
    BootstrapComplete,
    ConfigLoaded(String),
    UserDataLoaded(String),

    // Mirror selection events (race coordination)
    SelectFastestMirror,
    MirrorSelected(String),

    // Workflow events (sequential coordination)
    StartWorkflow { items: Vec<String> },
    WorkflowStepComplete(u32),
    WorkflowComplete,

    // Utility events
    LogMessage(String),
    Shutdown,
}

#[derive(Debug, Clone)]
enum AppEffect {
    LoadConfig,
    LoadUserData,
    TryMirror { url: String },
    ProcessWorkflowStep { step: u32, item: String },
    Cleanup,
    Log(String),
}

// ============================================================================
// Event Handler - Demonstrating New Effects API
// ============================================================================

fn handle_event(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, Storage<AppModel, EmptyStorage>>,
) -> Command<AppEvent, AppEffect> {
    let model = ctx.model_mut();

    match event {
        AppEvent::StartBootstrap => {
            model
                .messages
                .push("Starting application bootstrap...".to_string());

            // NEW API: Parallel coordination with barrier
            // Both effects run concurrently, barrier event emitted when both complete
            Effects::new([AppEffect::LoadConfig, AppEffect::LoadUserData])
                .parallel()
                .timeout_per(Duration::from_secs(10)) // 10s timeout per effect
                .label("bootstrap") // For tracing/debugging
                .barrier(AppEvent::BootstrapComplete) // Emit when all complete
        }

        AppEvent::BootstrapComplete => {
            model.bootstrap_complete = true;
            model
                .messages
                .push("Bootstrap completed successfully!".to_string());
            Command::none()
        }

        AppEvent::ConfigLoaded(config) => {
            model.config = Some(config.clone());
            model.messages.push(format!("Config loaded: {}", config));
            Command::none()
        }

        AppEvent::UserDataLoaded(data) => {
            model.user_data = Some(data.clone());
            model.messages.push(format!("User data loaded: {}", data));
            Command::none()
        }

        AppEvent::SelectFastestMirror => {
            model
                .messages
                .push("Selecting fastest mirror...".to_string());

            // NEW API: Race coordination with barrier
            // First effect to complete wins, others are cancelled, then barrier emitted
            Effects::new([
                AppEffect::TryMirror {
                    url: "https://mirror1.example.com".to_string(),
                },
                AppEffect::TryMirror {
                    url: "https://mirror2.example.com".to_string(),
                },
                AppEffect::TryMirror {
                    url: "https://mirror3.example.com".to_string(),
                },
            ])
            .race()
            .timeout_per(Duration::from_secs(5)) // 5s timeout per mirror
            .label("mirror-selection")
            .barrier(AppEvent::MirrorSelected("winner".to_string())) // Emit when first completes
        }

        AppEvent::MirrorSelected(mirror) => {
            model.fastest_mirror = Some(mirror.clone());
            model
                .messages
                .push(format!("Fastest mirror selected: {}", mirror));
            Command::none()
        }

        AppEvent::StartWorkflow { items } => {
            model
                .messages
                .push(format!("Starting workflow with {} items", items.len()));
            model.workflow_step = 0;

            // NEW API: Sequential coordination
            // Effects run one after another, barrier emitted when all complete
            let workflow_effects: Vec<AppEffect> = items
                .into_iter()
                .enumerate()
                .map(|(i, item)| AppEffect::ProcessWorkflowStep {
                    step: i as u32 + 1,
                    item,
                })
                .collect();

            Effects::new(workflow_effects)
                .sequence()
                .stop_on_error(true) // Stop if any step fails
                .label("workflow")
                .barrier(AppEvent::WorkflowComplete) // Emit when all steps complete
        }

        AppEvent::WorkflowStepComplete(step) => {
            model.workflow_step = step;
            model
                .messages
                .push(format!("Workflow step {} completed", step));
            Command::none()
        }

        AppEvent::WorkflowComplete => {
            model
                .messages
                .push("Workflow completed successfully!".to_string());
            Command::none()
        }

        AppEvent::LogMessage(message) => {
            model.messages.push(message);
            Command::none()
        }

        AppEvent::Shutdown => {
            model.messages.push("Shutting down...".to_string());

            // NEW API: Fire-and-forget parallel cleanup
            // Multiple cleanup tasks run in parallel, no barrier needed
            Effects::new([AppEffect::Cleanup]).parallel().spawn() // Fire-and-forget - don't wait for completion
        }
    }
}

// ============================================================================
// Effect Handlers
// ============================================================================

async fn handle_effects(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, EmptyStorage>,
) -> EffectResult<AppEvent> {
    match effect {
        AppEffect::LoadConfig => {
            println!("Loading application config...");

            // Simulate async config loading
            #[cfg(feature = "tokio")]
            tokio::time::sleep(Duration::from_millis(800)).await;

            let config = "app_config_v1.2.3".to_string();
            let _ = ctx.send_event(AppEvent::ConfigLoaded(config));
            EffectResult::None
        }

        AppEffect::LoadUserData => {
            println!("Loading user data...");

            // Simulate async user data loading
            #[cfg(feature = "tokio")]
            tokio::time::sleep(Duration::from_millis(1200)).await;

            let user_data = "user_12345_profile".to_string();
            let _ = ctx.send_event(AppEvent::UserDataLoaded(user_data));
            EffectResult::None
        }

        AppEffect::TryMirror { url } => {
            println!("Trying mirror: {}", url);

            // Simulate mirror response time (some are faster than others)
            let delay = if url.contains("mirror1") {
                Duration::from_millis(300) // Fastest
            } else if url.contains("mirror2") {
                Duration::from_millis(800) // Medium
            } else {
                Duration::from_millis(1500) // Slowest
            };

            #[cfg(feature = "tokio")]
            tokio::time::sleep(delay).await;

            // Winner takes all - only the first to complete will send this event
            let _ = ctx.send_event(AppEvent::MirrorSelected(url));
            EffectResult::None
        }

        AppEffect::ProcessWorkflowStep { step, item } => {
            println!("Processing workflow step {}: {}", step, item);

            // Simulate processing time
            #[cfg(feature = "tokio")]
            tokio::time::sleep(Duration::from_millis(400)).await;

            let _ = ctx.send_event(AppEvent::WorkflowStepComplete(step));
            EffectResult::None
        }

        AppEffect::Cleanup => {
            println!("Performing cleanup...");

            #[cfg(feature = "tokio")]
            tokio::time::sleep(Duration::from_millis(200)).await;

            let _ = ctx.send_event(AppEvent::LogMessage("Cleanup completed".to_string()));
            EffectResult::None
        }

        AppEffect::Log(message) => {
            println!("LOG: {}", message);
            EffectResult::None
        }
    }
}

// ============================================================================
// Demo Scenarios
// ============================================================================

#[cfg(feature = "examples")]
#[cfg(feature = "examples")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Effects Coordination Demo ===\n");

    // Build the application
    let (core, shell) = Syzygy::builder()
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effects)
        .build();
    let mut runner = Runner::new(core, shell);

    // Demo 1: Parallel coordination with barrier
    println!("Demo 1: Bootstrap (Parallel + Barrier)");
    println!("- Config and user data load in parallel");
    println!("- BootstrapComplete event emitted when both finish");
    println!("- 10s timeout per effect\n");

    runner.core().send_event(AppEvent::StartBootstrap)?;

    // Wait for bootstrap to complete
    for _ in 0..20 {
        let did_work = runner.tick(syzygy::spawn::spawner()).await?;
        if !did_work {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if runner.core().model().bootstrap_complete {
            break;
        }
    }

    println!(
        "Bootstrap status: {}\n",
        if runner.core().model().bootstrap_complete {
            "✓ Complete"
        } else {
            "✗ Failed"
        }
    );

    // Demo 2: Race coordination with barrier
    println!("Demo 2: Mirror Selection (Race + Barrier)");
    println!("- Three mirrors compete, first response wins");
    println!("- Losing mirrors are automatically cancelled");
    println!("- MirrorSelected event emitted for winner");
    println!("- 5s timeout per mirror\n");

    runner.core().send_event(AppEvent::SelectFastestMirror)?;

    // Wait for mirror selection
    for _ in 0..30 {
        let did_work = runner.tick(syzygy::spawn::spawner()).await?;
        if !did_work {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if runner.core().model().fastest_mirror.is_some() {
            break;
        }
    }

    println!(
        "Selected mirror: {:?}\n",
        runner.core().model().fastest_mirror
    );

    // Demo 3: Sequential coordination with barrier
    println!("Demo 3: Workflow Processing (Sequential + Barrier)");
    println!("- Items processed one after another");
    println!("- Stops on first error (configurable)");
    println!("- WorkflowComplete event emitted when all done\n");

    let workflow_items = vec![
        "validate_input".to_string(),
        "transform_data".to_string(),
        "save_results".to_string(),
        "send_notifications".to_string(),
    ];

    runner.core().send_event(AppEvent::StartWorkflow {
        items: workflow_items,
    })?;

    // Wait for workflow completion
    for _ in 0..30 {
        let did_work = runner.tick(syzygy::spawn::spawner()).await?;
        if !did_work {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if runner.core().model().workflow_step >= 4 {
            break;
        }
    }

    println!(
        "Workflow steps completed: {}\n",
        runner.core().model().workflow_step
    );

    // Demo 4: Fire-and-forget coordination
    println!("Demo 4: Shutdown (Fire-and-forget)");
    println!("- Cleanup tasks run in parallel");
    println!("- No waiting for completion");
    println!("- App can exit immediately\n");

    runner.core().send_event(AppEvent::Shutdown)?;

    // Quick cleanup tick but don't wait
    runner.tick(syzygy::spawn::spawner()).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Print final state
    println!("=== Final Application State ===");
    println!(
        "Bootstrap: {}",
        if runner.core().model().bootstrap_complete {
            "✓"
        } else {
            "✗"
        }
    );
    println!("Config: {:?}", runner.core().model().config);
    println!("User Data: {:?}", runner.core().model().user_data);
    println!(
        "Selected Mirror: {:?}",
        runner.core().model().fastest_mirror
    );
    println!("Workflow Steps: {}", runner.core().model().workflow_step);
    println!("\nMessage Log:");
    for (i, msg) in runner.core().model().messages.iter().enumerate() {
        println!("  {}. {}", i + 1, msg);
    }

    println!("\n=== Key Patterns Demonstrated ===");
    println!("✓ Parallel + Barrier: Wait for all effects to complete");
    println!("✓ Race + Barrier: First effect wins, others cancelled");
    println!("✓ Sequential + Barrier: Effects run in order");
    println!("✓ Fire-and-forget: Start effects but don't wait");
    println!("✓ Timeouts: Per-effect timeout configuration");
    println!("✓ Labels: Tracing/debugging support");

    Ok(())
}
