use syzygy::command::CommandStep;
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum DemoEvent {
    UserClicked,
    DataLoaded { data: String },
    UserNotFound,
    AuthRequired,
    ProcessComplete,
}

#[derive(Debug, Clone)]
enum DemoEffect {
    LoadUser { id: u32 },
    LoadUserPosts { id: u32 },
    ShowMessage { text: String },
    ProcessData { data: String },
}

#[derive(Debug, Default)]
struct DemoModel {
    user_authenticated: bool,
    user_id: Option<u32>,
    data: Option<String>,
}

use syzygy::storage::{EmptyStorage, Storage};

fn demo_update(
    event: DemoEvent,
    ctx: &mut EventContext<DemoEvent, DemoEffect, Storage<DemoModel, EmptyStorage>>,
) -> Command<DemoEvent, DemoEffect> {
    let model: &mut DemoModel = ctx.model_mut();
    match event {
        DemoEvent::UserClicked => {
            // Demonstrate monadic composition with conditional logic
            Command::effect(DemoEffect::LoadUser { id: 123 })
                .and_then(|outputs| {
                    // If load user command was issued, also load posts
                    if outputs.is_empty() {
                        Command::event(DemoEvent::UserNotFound)
                    } else {
                        Command::effect(DemoEffect::LoadUserPosts { id: 123 })
                    }
                })
                .when(model.user_authenticated) // Only if authenticated
                .or_else(Command::event(DemoEvent::AuthRequired)) // Fallback
                .then(Command::effect(DemoEffect::ShowMessage {
                    text: "Loading user data...".to_string(),
                }))
        }

        DemoEvent::DataLoaded { data } => {
            model.data = Some(data.clone());

            // Chain processing with filtering
            Command::batch([
                Command::effect(DemoEffect::ProcessData { data: data.clone() }),
                Command::effect(DemoEffect::ShowMessage {
                    text: "Processing...".to_string(),
                }),
                Command::event(DemoEvent::UserNotFound), // This will be filtered out
            ])
            .filter(|output| {
                // Remove error events from successful processing
                !matches!(output, CommandStep::Event(DemoEvent::UserNotFound))
            })
            .then(Command::event(DemoEvent::ProcessComplete))
        }

        DemoEvent::AuthRequired => {
            model.user_authenticated = false;
            Command::effect(DemoEffect::ShowMessage {
                text: "Authentication required".to_string(),
            })
        }

        DemoEvent::UserNotFound => Command::effect(DemoEffect::ShowMessage {
            text: "User not found".to_string(),
        }),

        DemoEvent::ProcessComplete => Command::effect(DemoEffect::ShowMessage {
            text: "Processing complete!".to_string(),
        }),
    }
}

fn main() {
    println!("Monadic Command Composition Demo");
    println!("=================================");

    let mut storage = syzygy::storage::EmptyStorage.with_model(DemoModel {
        user_authenticated: true,
        ..Default::default()
    });

    // Demonstrate complex monadic composition
    let mut ctx = EventContext::new(&mut storage);
    let command = demo_update(DemoEvent::UserClicked, &mut ctx);

    println!("Command created with {} outputs:", command.len());
    for (i, output) in command.into_iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            CommandStep::SequentialEffects(effects) => {
                println!(
                    "  {}: Sequential Effects - {} effects",
                    i + 1,
                    effects.len()
                );
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::ParallelEffects(effects) => {
                println!("  {}: Parallel Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
        }
    }

    // Demonstrate data processing with filtering
    let mut storage2 = syzygy::storage::EmptyStorage.with_model(DemoModel::default());
    let mut ctx2 = EventContext::new(&mut storage2);
    let command2 = demo_update(
        DemoEvent::DataLoaded {
            data: "test data".to_string(),
        },
        &mut ctx2,
    );

    println!("\nData processing command with {} outputs:", command2.len());
    for (i, output) in command2.into_iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            CommandStep::SequentialEffects(effects) => {
                println!(
                    "  {}: Sequential Effects - {} effects",
                    i + 1,
                    effects.len()
                );
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::ParallelEffects(effects) => {
                println!("  {}: Parallel Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
        }
    }

    println!("\nMonadic composition enables expressive, chainable command creation!");
}
