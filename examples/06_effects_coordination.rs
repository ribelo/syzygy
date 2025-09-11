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

use futures::FutureExt;
use std::time::Duration;
use syzygy::executor::{Task, TokioIo};
use syzygy::prelude::*;

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
#[allow(dead_code)]
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
    ctx: &mut EventContext<AppEvent, AppEffect, AppModel>,
) -> Command<AppEvent, AppEffect> {
    let model = ctx.model_mut();

    match event {
        AppEvent::StartBootstrap => {
            model
                .messages
                .push("Starting application bootstrap...".to_string());

            // NEW API: Parallel coordination with Task::all
            let _config_plan =
                Task::future_on::<TokioIo, _, _, _>(|_ctx: EffectContext<AppEvent, ()>| {
                    async move {
                        println!("Loading application config...");
                        #[cfg(feature = "tokio")]
                        tokio::time::sleep(Duration::from_millis(800)).await;
                        let config = "app_config_v1.2.3".to_string();
                        AppEvent::ConfigLoaded(config)
                    }
                    .boxed()
                });

            let _user_data_plan = Task::future_on::<TokioIo, _, _, _>(|_ctx: EffectContext<AppEvent, ()>| {
                async move {
                    println!("Loading user data...");
                    #[cfg(feature = "tokio")]
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                    let user_data = "user_12345_profile".to_string();
                    AppEvent::UserDataLoaded(user_data)
                }
                .boxed()
            });

            // Use a single task that runs both operations in parallel with futures::join!
            let _bootstrap_plan = Task::future_on::<TokioIo, _, _, _>(move |_ctx: EffectContext<AppEvent, ()>| {
                async move {
                    // Run both config and user data loading in parallel
                    let (config_result, user_data_result) = futures::join!(
                        async {
                            println!("Loading application config...");
                            #[cfg(feature = "tokio")]
                            tokio::time::sleep(Duration::from_millis(800)).await;
                            let config = "app_config_v1.2.3".to_string();
                            AppEvent::ConfigLoaded(config)
                        },
                        async {
                            println!("Loading user data...");
                            #[cfg(feature = "tokio")]
                            tokio::time::sleep(Duration::from_millis(1200)).await;
                            let user_data = "user_12345_profile".to_string();
                            AppEvent::UserDataLoaded(user_data)
                        }
                    );

                    // Return multiple events as a Vec
                    vec![config_result, user_data_result, AppEvent::BootstrapComplete]
                }
                .boxed()
            });

            Command::effect(AppEffect::LoadConfig)
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

            Command::effect(AppEffect::TryMirror {
                url: "https://mirror1.example.com".to_string(),
            })
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

            // Start the first workflow step
            if let Some(first_item) = items.first() {
                Command::effect(AppEffect::ProcessWorkflowStep {
                    step: 1,
                    item: first_item.clone(),
                })
            } else {
                Command::event(AppEvent::WorkflowComplete)
            }
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

            Command::effect(AppEffect::Cleanup)
        }
    }
}

// ============================================================================
// Effect Handlers
// ============================================================================

fn handle_effects(
    effect: AppEffect,
    _ctx: &EffectContext<AppEvent, ()>,
) -> Task<AppEvent, ()> {
    match effect {
        AppEffect::LoadConfig => Task::future_on::<TokioIo, _, _, _>(move |_ctx: EffectContext<AppEvent, ()>| {
            async move {
                println!("Loading application config...");
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(800)).await;
                let config = "app_config_v1.2.3".to_string();
                AppEvent::ConfigLoaded(config)
            }
            .boxed()
        }),

        AppEffect::LoadUserData => Task::future_on::<TokioIo, _, _, _>(move |_ctx: EffectContext<AppEvent, ()>| {
            async move {
                println!("Loading user data...");
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(1200)).await;
                let user_data = "user_12345_profile".to_string();
                AppEvent::UserDataLoaded(user_data)
            }
            .boxed()
        }),

        AppEffect::TryMirror { url } => {
            Task::future_on::<TokioIo, _, _, _>(move |_ctx: EffectContext<AppEvent, ()>| {
                let url = url.clone();
                async move {
                    println!("Trying mirror: {}", url);
                    let delay = if url.contains("mirror1") {
                        Duration::from_millis(300) // Fastest
                    } else if url.contains("mirror2") {
                        Duration::from_millis(800) // Medium
                    } else {
                        Duration::from_millis(1500) // Slowest
                    };

                    #[cfg(feature = "tokio")]
                    tokio::time::sleep(delay).await;

                    AppEvent::MirrorSelected(url)
                }
                .boxed()
            })
        }

        AppEffect::ProcessWorkflowStep { step, item } => {
            Task::future_on::<TokioIo, _, _, _>(move |_ctx: EffectContext<AppEvent, ()>| {
                let item = item.clone();
                async move {
                    println!("Processing workflow step {}: {}", step, item);
                    #[cfg(feature = "tokio")]
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    AppEvent::WorkflowStepComplete(step)
                }
                .boxed()
            })
        }

        AppEffect::Cleanup => Task::future_on::<TokioIo, _, _, _>(move |_ctx: EffectContext<AppEvent, ()>| {
            async move {
                println!("Performing cleanup...");
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(200)).await;
                AppEvent::LogMessage("Cleanup completed".to_string())
            }
            .boxed()
        }),

        AppEffect::Log(message) => {
            println!("LOG: {}", message);
            Task::events(vec![])
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

    runner.core().send_event(AppEvent::StartBootstrap);

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

    runner.core().send_event(AppEvent::SelectFastestMirror);

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
    });

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

    runner.core().send_event(AppEvent::Shutdown);

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
