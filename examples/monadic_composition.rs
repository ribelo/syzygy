use syzygy::prelude::*;
use syzygy::command::CommandStep;

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

#[derive(Default)]
struct DemoApp;

#[derive(Debug, Default)]
struct DemoModel {
    user_authenticated: bool,
    user_id: Option<u32>,
    data: Option<String>,
}

impl App for DemoApp {
    type Event = DemoEvent;
    type Model = DemoModel;
    type Effect = DemoEffect;
    type Resources = ();

    fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
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
                    .when(model.user_authenticated)  // Only if authenticated
                    .or_else(Command::event(DemoEvent::AuthRequired))  // Fallback
                    .then(Command::effect(DemoEffect::ShowMessage { 
                        text: "Loading user data...".to_string() 
                    }))
            }
            
            DemoEvent::DataLoaded { data } => {
                model.data = Some(data.clone());
                
                // Chain processing with filtering
                Command::batch([
                    Command::effect(DemoEffect::ProcessData { data: data.clone() }),
                    Command::effect(DemoEffect::ShowMessage { text: "Processing...".to_string() }),
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
                    text: "Authentication required".to_string() 
                })
            }
            
            DemoEvent::UserNotFound => {
                Command::effect(DemoEffect::ShowMessage { 
                    text: "User not found".to_string() 
                })
            }
            
            DemoEvent::ProcessComplete => {
                Command::effect(DemoEffect::ShowMessage { 
                    text: "Processing complete!".to_string() 
                })
            }
        }
    }
}

fn main() {
    println!("Monadic Command Composition Demo");
    println!("=================================");
    
    let app = DemoApp;
    let mut model = DemoModel {
        user_authenticated: true,
        ..Default::default()
    };
    
    // Demonstrate complex monadic composition
    let command = app.update(DemoEvent::UserClicked, &mut model);
    
    println!("Command created with {} outputs:", command.len());
        for (i, output) in command.into_iter().enumerate() {
            match output {
                CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
                CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            CommandStep::SequentialEffects(effects) => {
                println!("  {}: Sequential Effects - {} effects", i + 1, effects.len());
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
            CommandStep::InlineFuture(_) => println!("  {}: InlineFuture", i + 1),
        }
    }
    
    // Demonstrate data processing with filtering
    let mut model2 = DemoModel::default();
    let command2 = app.update(DemoEvent::DataLoaded { 
        data: "test data".to_string() 
    }, &mut model2);
    
    println!("\nData processing command with {} outputs:", command2.len());
    for (i, output) in command2.into_iter().enumerate() {
        match output {
            CommandStep::Event(event) => println!("  {}: Event - {:?}", i + 1, event),
            CommandStep::Effect(effect) => println!("  {}: Effect - {:?}", i + 1, effect),
            CommandStep::SequentialEffects(effects) => {
                println!("  {}: Sequential Effects - {} effects", i + 1, effects.len());
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
            CommandStep::InlineFuture(_) => println!("  {}: InlineFuture", i + 1),
        }
    }
    
    println!("\nMonadic composition enables expressive, chainable command creation!");
}
