//! Example: Monadic Command Composition with Magic Handlers
//!
//! This example demonstrates monadic command composition using magic handlers
//! for clean, focused event processing functions.

use syzygy::command::{CommandStep, GroupMode};
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
    LoadUser,
    LoadUserPosts,
    ShowMessage,
    ProcessData,
}

#[derive(Debug, Default)]
struct DemoModel {
    user_authenticated: bool,
    data: Option<String>,
    click_count: u32,
}



// ============================================================================
// Magic Event Handlers - Clean Monadic Composition
// ============================================================================

/// Handle user clicks with automatic model extraction and monadic composition
fn handle_user_click(event: DemoEvent, model: &mut DemoModel) -> Command<DemoEvent, DemoEffect> {
    if let DemoEvent::UserClicked = event {
        model.click_count += 1;
        
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
    } else {
        Command::none()
    }
}

/// Handle data loading with filtering and chaining
fn handle_data_loaded(event: DemoEvent, model: &mut DemoModel) -> Command<DemoEvent, DemoEffect> {
    if let DemoEvent::DataLoaded { data } = event {
        model.data = Some(data.clone());

        // Chain processing with filtering - demonstrates complex monadic composition
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
    } else {
        Command::none()
    }
}

/// Handle auth required events
fn handle_auth_required(event: DemoEvent, model: &mut DemoModel) -> Command<DemoEvent, DemoEffect> {
    if let DemoEvent::AuthRequired = event {
        model.user_authenticated = false;
        Command::effect(DemoEffect::ShowMessage)
    } else {
        Command::none()
    }
}

/// Handle simple events that just show messages
fn handle_simple_message(event: DemoEvent) -> Command<DemoEvent, DemoEffect> {
    match event {
        DemoEvent::UserNotFound | DemoEvent::ProcessComplete => {
            Command::effect(DemoEffect::ShowMessage)
        }
        _ => Command::none()
    }
}

// ============================================================================
// Main Update Function - Dispatches to Magic Handlers
// ============================================================================

fn demo_update(
    event: DemoEvent,
    ctx: &mut EventContext<DemoEvent, DemoEffect, Storage<DemoModel, EmptyStorage>>,
) -> Command<DemoEvent, DemoEffect> {
    // Dispatch to appropriate magic handler based on event type
    match event {
        DemoEvent::UserClicked => event_trigger(event, ctx, handle_user_click),
        DemoEvent::DataLoaded { .. } => event_trigger(event, ctx, handle_data_loaded),
        DemoEvent::AuthRequired => event_trigger(event, ctx, handle_auth_required),
        DemoEvent::UserNotFound | DemoEvent::ProcessComplete => {
            event_trigger(event, ctx, handle_simple_message)
        }
    }
}

fn main() {
    println!("Monadic Command Composition with Magic Handlers Demo");
    println!("===================================================");
    
    println!("This example demonstrates:");
    println!("• Monadic command composition (.and_then, .when, .or_else, .then)");
    println!("• Magic handlers with automatic parameter extraction");
    println!("• Complex command filtering and chaining");
    println!("• Clean separation of concerns\n");

    let mut storage = EmptyStorage.with_model(DemoModel {
        user_authenticated: true,
        click_count: 0,
        ..Default::default()
    });

    // Test 1: Authenticated user click (demonstrates complex monadic composition)
    println!("=== Test 1: Authenticated User Click ===");
    let mut ctx = EventContext::new(&mut storage);
    let command = demo_update(DemoEvent::UserClicked, &mut ctx);
    
    let model: &DemoModel = ctx.model();
    println!("Click count after processing: {}", model.click_count);
    println!("User authenticated: {}", model.user_authenticated);
    println!("Command created with {} outputs:", command.len());
    
    for (i, output) in command.into_iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            CommandStep::Batch(effects) => {
                println!("  {}: Sequential Effects - {} effects", i + 1, effects.len());
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
            _ => println!("  {}: Other command step", i + 1),
        }
    }

    // Test 2: Data processing with filtering (demonstrates command filtering)
    println!("\n=== Test 2: Data Processing with Filtering ===");
    let mut storage2 = EmptyStorage.with_model(DemoModel::default());
    let mut ctx2 = EventContext::new(&mut storage2);
    let command2 = demo_update(
        DemoEvent::DataLoaded {
            data: "test data".to_string(),
        },
        &mut ctx2,
    );
    
    let model2: &DemoModel = ctx2.model();
    println!("Data loaded: {:?}", model2.data);
    println!("Filtered command with {} outputs:", command2.len());
    
    for (i, output) in command2.into_iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            CommandStep::Batch(effects) => {
                println!("  {}: Sequential Effects - {} effects", i + 1, effects.len());
                for (j, effect) in effects.iter().enumerate() {
                    println!("    {}.{}: {:?}", i + 1, j + 1, effect);
                }
            }
            _ => println!("  {}: Other command step", i + 1),
        }
    }
    
    // Test 3: Unauthenticated user (demonstrates .or_else fallback)
    println!("\n=== Test 3: Unauthenticated User (Fallback) ===");
    let mut storage3 = EmptyStorage.with_model(DemoModel {
        user_authenticated: false,
        ..Default::default()
    });
    let mut ctx3 = EventContext::new(&mut storage3);
    let command3 = demo_update(DemoEvent::UserClicked, &mut ctx3);
    
    println!("Unauthenticated command with {} outputs:", command3.len());
    for (i, output) in command3.into_iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            _ => println!("  {}: Other command step", i + 1),
        }
    }

    println!("\n=== Magic Handlers + Monadic Composition Key Points ===");
    println!("✅ Clean handler functions with automatic parameter extraction");
    println!("✅ Expressive monadic composition (.and_then, .when, .or_else, .filter)");
    println!("✅ Type-safe conditional command creation");
    println!("✅ Complex command filtering and chaining");
    println!("✅ Zero runtime overhead - compiles to direct function calls");
    println!("✅ Separation of concerns - each handler focuses on one event type");
}
