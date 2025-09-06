//! Example showing the new storage-based API without App trait
//!
//! This example demonstrates:
//! - Multiple models in type-safe storage
//! - Direct update function instead of App trait
//! - Runtime multi-model access
//! - Clean builder pattern

use syzygy::event_context::EventContext;
use syzygy::prelude::*;
use syzygy::streaming::EffectResult;

// Define our events
#[derive(Debug, Clone)]
enum AppEvent {
    UpdateUser { name: String, email: String },
    ChangeTheme { theme: String },
    ToggleNotifications,
    IncrementCounter,
}

// Define our effects
#[derive(Debug, Clone)]
enum AppEffect {
    SaveUser { name: String, email: String },
    SaveConfig { theme: String, notifications: bool },
    LogMessage { message: String },
}

// Define our models
#[derive(Debug, Default, Clone)]
struct UserModel {
    name: String,
    email: String,
    login_count: u32,
}

#[derive(Debug, Default, Clone)]
struct ConfigModel {
    theme: String,
    notifications: bool,
    version: String,
}

#[derive(Debug, Default, Clone)]
struct CounterModel {
    value: i32,
}

// Define our update function - this replaces the App trait
fn update(
    event: AppEvent,
    ctx: &mut EventContext<
        AppEvent,
        AppEffect,
        Storage<CounterModel, Storage<ConfigModel, Storage<UserModel, EmptyStorage>>>,
    >,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UpdateUser { name, email } => {
            println!("Updating user: {name} <{email}>");
            {
                let user: &mut UserModel = ctx.model_mut();
                user.name = name.clone();
                user.email = email.clone();
                user.login_count += 1;
            }

            // Return effect to save user
            Command::effect(AppEffect::SaveUser { name, email })
        }

        AppEvent::ChangeTheme { theme } => {
            println!("Changing theme to: {theme}");
            let notifications = {
                let config: &mut ConfigModel = ctx.model_mut();
                config.theme = theme.clone();
                config.notifications
            };

            // Return effect to save config
            Command::effect(AppEffect::SaveConfig {
                theme,
                notifications,
            })
        }

        AppEvent::ToggleNotifications => {
            let (theme, notifications) = {
                let config: &mut ConfigModel = ctx.model_mut();
                config.notifications = !config.notifications;
                println!(
                    "Notifications: {}",
                    if config.notifications { "ON" } else { "OFF" }
                );
                (config.theme.clone(), config.notifications)
            };

            Command::effect(AppEffect::SaveConfig {
                theme,
                notifications,
            })
        }

        AppEvent::IncrementCounter => {
            let counter_value = {
                let counter: &mut CounterModel = ctx.model_mut();
                counter.value += 1;
                counter.value
            };
            println!("Counter: {counter_value}");

            Command::effect(AppEffect::LogMessage {
                message: format!("Counter incremented to {counter_value}"),
            })
        }
    }
}

// Effect handler
async fn handle_effects(effect: AppEffect, _ctx: EffectContext<AppEvent, EmptyStorage>) -> EffectResult<AppEvent> {
    match effect {
        AppEffect::SaveUser { name, email } => {
            println!("💾 Saving user: {name} <{email}>");
            // Simulate async work
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            println!("✅ User saved successfully");
        }

        AppEffect::SaveConfig {
            theme,
            notifications,
        } => {
            println!("💾 Saving config: theme={theme}, notifications={notifications}");
            // Simulate async work
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            println!("✅ Config saved successfully");
        }

        AppEffect::LogMessage { message } => {
            println!("📝 LOG: {message}");
        }
    }
    EffectResult::None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Syzygy Storage API Example");

    // Build the system with multiple models
    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(UserModel {
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            login_count: 0,
        })
        .model(ConfigModel {
            theme: "light".to_string(),
            notifications: true,
            version: "1.0.0".to_string(),
        })
        .model(CounterModel { value: 0 })
        .event_handler(update)
        .effect_handler(handle_effects)
        .build();

    // Effect handler provided via builder

    // Create runner for orchestration
    let mut runner = Runner::new(core, shell);

    println!("📊 Initial state:");
    let config: &ConfigModel = runner.core().storage().get();
    let counter: &CounterModel = runner.core().storage().get();

    println!(
        "  User: {} <{}> (logins: {})",
        user.name, user.email, user.login_count
    );
    println!(
        "  Config: theme={}, notifications={}, version={}",
        config.theme, config.notifications, config.version
    );
    println!("  Counter: {}", counter.value);

    println!("\n🎯 Sending events...");

    // Send various events
    runner.core_mut().send_event(AppEvent::UpdateUser {
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
    })?;

    runner.core_mut().send_event(AppEvent::ChangeTheme {
        theme: "dark".to_string(),
    })?;

    runner
        .core_mut()
        .send_event(AppEvent::ToggleNotifications)?;

    for _i in 1..=3 {
        runner.core_mut().send_event(AppEvent::IncrementCounter)?;
    }

    // Process events and effects
    println!("\n⚙️  Processing events and effects...");

    for _ in 0..10 {
        let processed = runner.tick(syzygy::spawn::spawner()).await?;
        if !processed {
            break;
        }

        // Small delay to see the async effects
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    println!("\n📊 Final state:");
    let config: &ConfigModel = runner.core().storage().get();
    let counter: &CounterModel = runner.core().storage().get();

    println!(
        "  User: {} <{}> (logins: {})",
        user.name, user.email, user.login_count
    );
    println!(
        "  Config: theme={}, notifications={}, version={}",
        config.theme, config.notifications, config.version
    );
    println!("  Counter: {}", counter.value);

    println!("\n✨ Example completed successfully!");

    Ok(())
}
