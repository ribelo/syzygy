//! Demonstration of Command's domain-specific operations
//!
//! This example shows how Command provides Iterator-like functionality
//! without implementing the Iterator trait, maintaining type safety
//! and semantic clarity.

use syzygy::command::{CommandStep, GroupMode};
use syzygy::event_context::EventContext;
use syzygy::prelude::*;
use syzygy::storage::StorageBuilder;

#[derive(Debug, Clone, PartialEq)]
enum UserEvent {
    Login { username: String },
    ViewProfile { user_id: u32 },
    UpdateProfile { user_id: u32, data: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq)]
enum UserEffect {
    ValidateCredentials { username: String },
    LoadUserData { user_id: u32 },
    SaveUserData { user_id: u32, data: String },
    SendNotification { message: String },
    LogActivity { activity: String },
}

#[derive(Debug, Default)]
struct UserModel {
    authenticated: bool,
}

use syzygy::storage::{EmptyStorage, Storage};

fn user_update(
    event: UserEvent,
    ctx: &mut EventContext<UserEvent, UserEffect, Storage<UserModel, EmptyStorage>>,
) -> Command<UserEvent, UserEffect> {
    let model: &mut UserModel = ctx.model_mut();
    match event {
        UserEvent::Login { username } => {
            Command::effect(UserEffect::ValidateCredentials { username }).then(Command::effect(
                UserEffect::LogActivity {
                    activity: "Login attempt".to_string(),
                },
            ))
        }
        UserEvent::ViewProfile { user_id } => {
            if model.authenticated {
                Command::batch([
                    Command::effect(UserEffect::LoadUserData { user_id }),
                    Command::effect(UserEffect::LogActivity {
                        activity: format!("Viewed profile {user_id}"),
                    }),
                ])
            } else {
                Command::event(UserEvent::Error {
                    message: "Authentication required".to_string(),
                })
            }
        }
        UserEvent::UpdateProfile { user_id, data } => {
            Command::batch([
                Command::effect(UserEffect::SaveUserData { user_id, data }),
                Command::effect(UserEffect::SendNotification {
                    message: "Profile updated".to_string(),
                }),
                Command::event(UserEvent::ViewProfile { user_id }), // Refresh view
            ])
        }
        UserEvent::Error { message } => Command::effect(UserEffect::LogActivity {
            activity: format!("Error: {message}"),
        }),
    }
}

fn main() {
    println!("Command Domain-Specific Operations Demo");
    println!("======================================");

    let mut storage = syzygy::storage::EmptyStorage::new().with_model(UserModel {
        authenticated: true,
    });

    // Create a complex command
    let mut ctx = EventContext::new(&mut storage);
    let command = user_update(
        UserEvent::UpdateProfile {
            user_id: 123,
            data: "New profile data".to_string(),
        },
        &mut ctx,
    );

    println!("\n1. Command Analysis:");
    println!("   Total outputs: {}", command.len());
    println!("   Events: {}", command.count_events());
    println!("   Effects: {}", command.count_effects());
    println!("   Has events: {}", command.has_events());
    println!("   Has effects: {}", command.has_effects());

    // Non-consuming iteration for inspection
    println!("\n2. Non-consuming inspection:");
    for (i, output) in command.iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("   {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("   {}: Effect - {:?}", i + 1, effect),
            CommandStep::Batch(effects) => {
                println!(
                    "   {}: Sequential Effects - {} effects",
                    i + 1,
                    effects.len()
                );
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None } => {
                println!("   {}: Parallel Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Group { effects, mode: GroupMode::Race, .. } => {
                println!("   {}: Race Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Group { effects, .. } => {
                println!("   {}: Group Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Merge { effects, barrier_event } => {
                println!("   {}: Merge Effects - {} effects, barrier: {:?}", i + 1, effects.len(), barrier_event);
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Join { effects, timeout_per, barrier_event } => {
                println!("   {}: Join Effects - {} effects, timeout: {:?}, barrier: {:?}", i + 1, effects.len(), timeout_per, barrier_event);
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Race { effects, timeout_per, barrier_event } => {
                println!("   {}: Race Effects - {} effects, timeout: {:?}, barrier: {:?}", i + 1, effects.len(), timeout_per, barrier_event);
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Chain { effects, barrier_event } => {
                println!("   {}: Chain Effects - {} effects, barrier: {:?}", i + 1, effects.len(), barrier_event);
                for (j, effect) in effects.iter().enumerate() {
                    println!("     {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
        }
    }

    // Domain-specific searching
    println!("\n3. Type-safe searching:");
    if let Some(view_event) = command.find_event(|e| matches!(e, UserEvent::ViewProfile { .. })) {
        println!("   Found view event: {view_event:?}");
    }

    if let Some(save_effect) = command.find_effect(|e| matches!(e, UserEffect::SaveUserData { .. }))
    {
        println!("   Found save effect: {save_effect:?}");
    }

    // Partition by type for separate processing
    println!("\n4. Partitioning outputs:");
    let (events, effects) = command.partition_outputs();
    println!(
        "   Separated {} events and {} effects",
        events.len(),
        effects.len()
    );

    println!("   Events to process:");
    for event in &events {
        println!("     - {event:?}");
    }

    println!("   Effects to execute:");
    for effect in &effects {
        println!("     - {effect:?}");
    }

    // Demonstrate monadic composition with domain operations
    println!("\n5. Combined monadic and domain operations:");
    let enhanced_command = Command::event(UserEvent::Login {
        username: "alice".to_string(),
    })
    .and_then(|outputs: &[CommandStep<UserEvent, UserEffect>]| {
        if outputs.is_empty() {
            Command::event(UserEvent::Error {
                message: "Login failed".to_string(),
            })
        } else {
            Command::event(UserEvent::ViewProfile { user_id: 123 })
        }
    })
    .when(true); // Only if condition is met

    println!("   Enhanced command has {} outputs", enhanced_command.len());
    println!("   Contains {} events", enhanced_command.count_events());

    // Try fallible transformation
    println!("\n6. Fallible event transformation:");
    let login_command: Command<UserEvent, UserEffect> = Command::event(UserEvent::Login {
        username: "alice".to_string(),
    });

    let validated_result = login_command.try_map_event(|event| match event {
        UserEvent::Login { username } if username.len() >= 3 => {
            Ok(UserEvent::ViewProfile { user_id: 123 })
        }
        UserEvent::Login { .. } => Err("Username too short"),
        other => Ok(other),
    });

    match validated_result {
        Ok(validated_command) => {
            println!(
                "   Validation successful: {} outputs",
                validated_command.len()
            );
        }
        Err(error) => {
            println!("   Validation failed: {error}");
        }
    }

    println!("\n7. Why this is better than Iterator trait:");
    println!("   ✓ Type safety: Events and Effects are distinguished");
    println!("   ✓ Domain clarity: Methods have clear semantics");
    println!("   ✓ Performance: Optimized for small collections (SmallVec)");
    println!("   ✓ Non-consuming: Can inspect without losing ownership");
    println!("   ✓ Fail-fast: try_map_* methods stop at first error");
    println!("   ✓ Monadic composition: Conditional logic with and_then()");

    println!("\nCommand domain operations provide Iterator-like functionality");
    println!("with better type safety and semantic clarity for effect orchestration!");
}
