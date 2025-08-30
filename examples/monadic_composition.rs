use syzygy::command::{CommandStep, GroupMode};
use syzygy::prelude::*;
use syzygy::storage::StorageBuilder;

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
    LoadUser,
    LoadUserPosts,
    ShowMessage,
    ProcessData,
}

#[derive(Debug, Default)]
struct DemoModel {
    user_authenticated: bool,
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
            Command::effect(DemoEffect::LoadUser)
                .and_then(|outputs| {
                    // If load user command was issued, also load posts
                    if outputs.is_empty() {
                        Command::event(DemoEvent::UserNotFound)
                    } else {
                        Command::effect(DemoEffect::LoadUserPosts)
                    }
                })
                .when(model.user_authenticated) // Only if authenticated
                .or_else(Command::event(DemoEvent::AuthRequired)) // Fallback
                .then(Command::effect(DemoEffect::ShowMessage))
        }

        DemoEvent::DataLoaded { data } => {
            model.data = Some(data.clone());

            // Chain processing with filtering
            Command::batch([
                Command::effect(DemoEffect::ProcessData),
                Command::effect(DemoEffect::ShowMessage),
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
            Command::effect(DemoEffect::ShowMessage)
        }

        DemoEvent::UserNotFound => Command::effect(DemoEffect::ShowMessage),

        DemoEvent::ProcessComplete => Command::effect(DemoEffect::ShowMessage),
    }
}

fn main() {
    println!("Monadic Command Composition Demo");
    println!("=================================");

    let mut storage = syzygy::storage::EmptyStorage::new().with_model(DemoModel {
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
            CommandStep::Batch(effects) => {
                println!(
                    "  {}: Sequential Effects - {} effects",
                    i + 1,
                    effects.len()
                );
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None } => {
                println!("  {}: Parallel Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
        }
    }

    // Demonstrate data processing with filtering
    let mut storage2 = syzygy::storage::EmptyStorage::new().with_model(DemoModel::default());
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
            CommandStep::Batch(effects) => {
                println!(
                    "  {}: Sequential Effects - {} effects",
                    i + 1,
                    effects.len()
                );
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None } => {
                println!("  {}: Parallel Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
        }
    }

    println!("\nMonadic composition enables expressive, chainable command creation!");
}
