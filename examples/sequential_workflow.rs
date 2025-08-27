//! # Sequential Workflow Example
//!
//! This example demonstrates the sequential workflow pattern that solves the
//! "200+ lines of event spaghetti" problem. Instead of complex event chains,
//! effects can be composed sequentially with automatic error handling.
//!
//! **Before** (event-driven spaghetti):
//! ```ignore
//! LoginUser → event → FetchUserData → event → FetchAddressData → event → MakeASandwichForUser
//! ```
//! Requires manual state tracking, error handling, and complex event orchestration.
//!
//! **After** (sequential effects):
//! ```ignore
//! Command::sequence_effects([
//!     LoginUser { credentials },
//!     FetchUserData { user_id },
//!     FetchAddressData { user_id },
//!     MakeASandwichForUser { user_id, preferences },
//! ])
//! ```
//! Clean, readable, automatic error handling, zero boilerplate.

use std::time::Duration;
use syzygy::prelude::*;
use tokio::time::sleep;

#[derive(Debug, Clone)]
enum WorkflowEvent {
    StartUserOnboarding { username: String, password: String },
    ErrorOccurred { message: String },
    OnboardingComplete { user_id: u32 },
}

#[derive(Debug, Clone)]
enum WorkflowEffect {
    LoginUser { username: String, password: String },
    FetchUserData { user_id: u32 },
    FetchAddressData { user_id: u32 },
    MakeASandwichForUser { user_id: u32, preferences: String },
    // Parallel effects for comparison
    FetchUserProfile { user_id: u32 },
    FetchUserNotifications { user_id: u32 },
    FetchUserPreferences { user_id: u32 },
}

#[derive(Default)]
struct WorkflowModel {
    user_id: Option<u32>,
    completed_steps: Vec<String>,
    errors: Vec<String>,
}

use syzygy::storage::{EmptyStorage, Storage};

fn workflow_update(
    event: WorkflowEvent,
    ctx: &mut EventContext<WorkflowEvent, WorkflowEffect, Storage<WorkflowModel, EmptyStorage>>,
) -> Command<WorkflowEvent, WorkflowEffect> {
    let model: &mut WorkflowModel = ctx.model_mut();

    match event {
        WorkflowEvent::StartUserOnboarding { username, password } => {
            println!("🚀 Starting user onboarding for: {username}");

            // This is the key innovation: sequential effects with fail-fast error handling
            // If any step fails, the entire pipeline stops automatically
            Command::sequence([
                Command::effect(WorkflowEffect::LoginUser {
                    username: username.clone(),
                    password,
                }),
                Command::effect(WorkflowEffect::FetchUserData {
                    user_id: 42, // In real app, this would come from login result
                }),
                Command::effect(WorkflowEffect::FetchAddressData { user_id: 42 }),
                Command::effect(WorkflowEffect::MakeASandwichForUser {
                    user_id: 42,
                    preferences: "Turkey and swiss".to_string(),
                }),
            ])
        }

        WorkflowEvent::ErrorOccurred { message } => {
            println!("❌ Error: {message}");
            model.errors.push(message);
            Command::none()
        }

        WorkflowEvent::OnboardingComplete { user_id } => {
            println!("✅ Onboarding complete for user {user_id}");
            model.user_id = Some(user_id);

            // After successful onboarding, fetch additional data
            // Since we removed parallel coordination, we'll use individual effects
            // (In a real app, you might batch these or use a different pattern)
            println!("📊 Fetching additional user data...");
            Command::batch([
                Command::effect(WorkflowEffect::FetchUserProfile { user_id }),
                Command::effect(WorkflowEffect::FetchUserNotifications { user_id }),
                Command::effect(WorkflowEffect::FetchUserPreferences { user_id }),
            ])
        }
    }
}

/// Mock effect handler that simulates realistic async operations
async fn create_effect_handler(
    effect: WorkflowEffect,
    ctx: EffectContext<WorkflowEvent, EmptyStorage>,
) {
    match effect {
        WorkflowEffect::LoginUser {
            username,
            password: _,
        } => {
            println!("🔐 Logging in user: {username}");
            sleep(Duration::from_millis(100)).await;

            // Simulate successful login
            let _ = ctx.send_event(WorkflowEvent::OnboardingComplete { user_id: 42 });
        }

        WorkflowEffect::FetchUserData { user_id } => {
            println!("👤 Fetching user data for user {user_id}");
            sleep(Duration::from_millis(150)).await;
            // In a real app, this might fail and send ErrorOccurred event
        }

        WorkflowEffect::FetchAddressData { user_id } => {
            println!("🏠 Fetching address data for user {user_id}");
            sleep(Duration::from_millis(120)).await;
        }

        WorkflowEffect::MakeASandwichForUser {
            user_id,
            preferences,
        } => {
            println!("🥪 Making sandwich for user {user_id}: {preferences}");
            sleep(Duration::from_millis(200)).await;
        }

        // Parallel effects
        WorkflowEffect::FetchUserProfile { user_id } => {
            println!("📋 Fetching profile for user {user_id}");
            sleep(Duration::from_millis(80)).await;
        }

        WorkflowEffect::FetchUserNotifications { user_id } => {
            println!("🔔 Fetching notifications for user {user_id}");
            sleep(Duration::from_millis(60)).await;
        }

        WorkflowEffect::FetchUserPreferences { user_id } => {
            println!("⚙️ Fetching preferences for user {user_id}");
            sleep(Duration::from_millis(90)).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌟 Syzygy Sequential Workflow Example");
    println!("=====================================");
    println!();
    println!("This example demonstrates how Syzygy's sequential effects");
    println!("solve the '200+ lines of event spaghetti' problem.");
    println!();

    // Build the application
    let (core, shell) = Syzygy::builder::<WorkflowEvent, WorkflowEffect>()
        .model(WorkflowModel::default())
        .update(workflow_update)
        .build();

    let shell = shell.with_effect_handler(create_effect_handler);
    let mut runner = Runner::new(core, shell);

    // Start the workflow
    runner
        .core()
        .send_event(WorkflowEvent::StartUserOnboarding {
            username: "alice".to_string(),
            password: "secure123".to_string(),
        })?;

    // Run the application for a few seconds
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        runner.tick(syzygy::spawn::spawner()).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    println!();
    println!("🎉 Workflow demonstration complete!");
    println!();
    println!("Key benefits demonstrated:");
    println!(
        "• Sequential execution: LoginUser → FetchUserData → FetchAddressData → MakeASandwichForUser"
    );
    println!("• Automatic error handling: Any step failure stops the pipeline");
    println!("• Zero boilerplate: No manual state tracking or event orchestration");
    println!("• Simplified approach: Focus on sequential patterns that solve the core problem");
    println!("• Clean, readable code: Compare this to 200+ lines of event spaghetti!");

    Ok(())
}
