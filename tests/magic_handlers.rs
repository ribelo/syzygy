#![allow(clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args)]
//! Comprehensive tests for the magic handler system
//!
//! This test file verifies that the magic handler system works correctly
//! with the current EventContext and EffectContext design.

use syzygy::command::Command;
use syzygy::event_context::EventContext;
use syzygy::magic_handler::event_trigger;
use syzygy::storage::{EmptyStorage, StorageBuilder};

// ============================================================================
// Test Types
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserModel {
    name: String,
    count: i32,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct ConfigModel {
    theme: String,
    enabled: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum AppEvent {
    UserCreated { name: String },
    ConfigUpdated { theme: String },
    DataLoaded { data: String },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum AppEffect {
    SaveUser { name: String },
    LogMessage { message: String },
    FetchData { id: u32 },
}

// ============================================================================
// Event Magic Handler Tests
// ============================================================================

#[test]
fn test_event_only_handler() {
    let mut storage = EmptyStorage::new().with_model(UserModel::default());
    let ctx = EventContext::<AppEvent, AppEffect, _>::new(&mut storage);

    fn test_handler(event: AppEvent) -> Command<AppEvent, AppEffect> {
        match event {
            AppEvent::UserCreated { .. } => Command::effect(AppEffect::LogMessage {
                message: "User created via magic handler".to_string(),
            }),
            _ => Command::none(),
        }
    }

    let event = AppEvent::UserCreated {
        name: "Eve".to_string(),
    };

    let command = event_trigger(event, &ctx, test_handler);
    let steps: Vec<_> = command.into_iter().collect();
    assert_eq!(steps.len(), 1);
}

#[test]
fn test_event_with_model_mut_extraction() {
    let mut storage = EmptyStorage::new().with_model(UserModel {
        name: "Dave".to_string(),
        count: 99,
    });

    let ctx = EventContext::<AppEvent, AppEffect, _>::new(&mut storage);

    // Handler that extracts mutable model reference
    fn handler_with_model_mut(
        event: AppEvent,
        user: &mut UserModel,
    ) -> Command<AppEvent, AppEffect> {
        match event {
            AppEvent::DataLoaded { .. } => {
                let message = format!(
                    "Data loaded for user: {} (count: {})",
                    user.name, user.count
                );
                Command::effect(AppEffect::LogMessage { message })
            }
            _ => Command::none(),
        }
    }

    let event = AppEvent::DataLoaded {
        data: "test".to_string(),
    };
    let command = event_trigger(event, &ctx, handler_with_model_mut);

    let steps: Vec<_> = command.into_iter().collect();
    assert_eq!(steps.len(), 1);
}

#[test]
fn test_magic_handler_flexibility() {
    let mut storage = EmptyStorage::new().with_model(UserModel::default());
    let ctx = EventContext::<AppEvent, AppEffect, _>::new(&mut storage);

    fn test_handler(event: AppEvent) -> Command<AppEvent, AppEffect> {
        match event {
            AppEvent::UserCreated { .. } => Command::effect(AppEffect::LogMessage {
                message: "User created via magic handler".to_string(),
            }),
            _ => Command::none(),
        }
    }

    let event = AppEvent::UserCreated {
        name: "Eve".to_string(),
    };

    let command = event_trigger(event, &ctx, test_handler);
    let steps: Vec<_> = command.into_iter().collect();
    assert_eq!(steps.len(), 1);

    // Test passes if it compiles and runs without error
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_event_handler_with_multiple_model_extraction() {
    // Setup storage with multiple models
    let mut event_storage = EmptyStorage::new()
        .with_model(UserModel {
            name: "initial".to_string(),
            count: 0,
        })
        .with_model(ConfigModel {
            theme: "dark".to_string(),
            enabled: false,
        });

    // Event handler that uses both models
    fn update_handler(
        event: AppEvent,
        user: &UserModel,
        config: &ConfigModel,
    ) -> Command<AppEvent, AppEffect> {
        match event {
            AppEvent::UserCreated { name } => {
                let _message = format!(
                    "Processing user creation: {} (current: {} count: {}, theme: {})",
                    name, user.name, user.count, config.theme
                );
                Command::effect(AppEffect::SaveUser { name })
            }
            _ => Command::none(),
        }
    }

    // Run the workflow
    let event_ctx = EventContext::<AppEvent, AppEffect, _>::new(&mut event_storage);

    let event = AppEvent::UserCreated {
        name: "Integration User".to_string(),
    };

    // Process event with magic handler
    let command = event_trigger(event, &event_ctx, update_handler);

    let steps: Vec<_> = command.into_iter().collect();
    assert_eq!(steps.len(), 1);

    // Test passes if workflow completes successfully
}
