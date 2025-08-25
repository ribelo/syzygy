//! Example: Magic Handler Derive Usage
//!
//! Shows how to use the #[derive(MagicVariants)] macro for clean magic handler setup.
//! This generates all the boilerplate variant structs and From implementations.

use syzygy::prelude::*;

#[derive(Debug, Clone)]
struct ButtonClick {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone)]
struct KeyPress {
    key: String,
}

#[derive(Debug, Clone)]
struct UserLogin {
    username: String,
}

#[derive(Debug, Clone)]
struct DataReceived {
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Error {
    message: String,
}

#[derive(Debug, Clone, MagicVariants)]
enum MyEvent {
    ButtonClick(ButtonClick),
    KeyPress(KeyPress),
    UserLogin(UserLogin),
    DataReceived(DataReceived),
    Error(Error),
}

#[derive(Debug, Clone)]
struct LogMessage {
    text: String,
}

#[derive(Debug, Clone)]
struct SaveToDatabase {
    data: String,
}

#[derive(Debug, Clone)]
struct PlaySound {
    sound: String,
}

#[derive(Debug, Clone, MagicVariants)]
enum MyEffect {
    LogMessage(LogMessage),
    SaveToDatabase(SaveToDatabase),
    PlaySound(PlaySound),
}

#[derive(Debug, Default)]
struct MyModel {
    username: Option<String>,
    click_count: u32,
    last_key: Option<String>,
    error_message: Option<String>,
}

// Magic handlers - these use event variants for automatic extraction
fn handle_button_click(
    ButtonClick { x, y }: ButtonClick,
    model: &mut MyModel,
) -> Command<MyEvent, MyEffect> {
    model.click_count += 1;
    Command::effect(MyEffect::LogMessage(LogMessage {
        text: format!(
            "Button clicked at ({}, {}) - total clicks: {}",
            x, y, model.click_count
        ),
    }))
}

fn handle_key_press(
    KeyPress { key }: KeyPress,
    model: &mut MyModel,
) -> Command<MyEvent, MyEffect> {
    model.last_key = Some(key.clone());
    if key == "Enter" {
        Command::batch(vec![
            Command::effect(MyEffect::PlaySound(PlaySound {
                sound: "ding.wav".to_string(),
            })),
            Command::effect(MyEffect::LogMessage(LogMessage {
                text: "Enter pressed!".to_string(),
            })),
        ])
    } else {
        Command::none()
    }
}

fn handle_user_login(
    UserLogin { username }: UserLogin,
    model: &mut MyModel,
) -> Command<MyEvent, MyEffect> {
    model.username = Some(username.clone());
    model.error_message = None;
    Command::batch(vec![
        Command::effect(MyEffect::SaveToDatabase(SaveToDatabase {
            data: format!("User {} logged in", username),
        })),
        Command::effect(MyEffect::LogMessage(LogMessage {
            text: format!("Welcome, {}!", username),
        })),
    ])
}

fn handle_data_received(DataReceived { data }: DataReceived) -> Command<MyEvent, MyEffect> {
    let data_str = String::from_utf8_lossy(&data);
    Command::effect(MyEffect::LogMessage(LogMessage {
        text: format!("Received {} bytes: {}", data.len(), data_str),
    }))
}

fn handle_error(
    Error { message }: Error,
    model: &mut MyModel,
) -> Command<MyEvent, MyEffect> {
    model.error_message = Some(message.clone());
    Command::effect(MyEffect::LogMessage(LogMessage {
        text: format!("Error: {}", message),
    }))
}


// Variant types and From implementations are generated automatically by #[derive(MagicVariants)]!

// CLEAN MAGIC HANDLER DISPATCH
fn handle_events(
    event: MyEvent,
    ctx: &mut EventContext<MyEvent, MyEffect, Storage<MyModel, EmptyStorage>>,
) -> Command<MyEvent, MyEffect> {
    event_magic_handler!(event, ctx, MyEvent {
        ButtonClick => handle_button_click,
        KeyPress => handle_key_press,
        UserLogin => handle_user_login,
        DataReceived => handle_data_received,
        Error => handle_error,
    })
}

// Example showing the same magic handler pattern - clean and consistent
fn handle_events_with_logging(
    event: MyEvent,
    ctx: &mut EventContext<MyEvent, MyEffect, Storage<MyModel, EmptyStorage>>,
) -> Command<MyEvent, MyEffect> {
    event_magic_handler!(event, ctx, MyEvent {
        ButtonClick => handle_button_click,
        KeyPress => handle_key_press,
        UserLogin => handle_user_login,
        DataReceived => handle_data_received,
        Error => handle_error,
    })
}

// Magic effect handlers
async fn handle_log_message(LogMessage { text }: LogMessage) {
    println!("[LOG] {}", text);
}

async fn handle_save_to_database(SaveToDatabase { data }: SaveToDatabase) {
    println!("[DB] Saving: {}", data);
}

async fn handle_play_sound(PlaySound { sound }: PlaySound) {
    println!("[SOUND] Playing: {}", sound);
}

async fn handle_effects(effect: MyEffect, ctx: EffectContext<MyEvent, EmptyStorage>) {
    effect_magic_handler!(effect, ctx, MyEffect {
        LogMessage => handle_log_message,
        SaveToDatabase => handle_save_to_database,
        PlaySound => handle_play_sound,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Magic Handlers Macro ===");

    // Test with regular handler (uses Command::none() for unmatched)
    {
        let (core, shell) = Syzygy::builder()
            .model(MyModel::default())
            .update(handle_events)
            .build();

        let shell = shell.with_effect_handler(handle_effects);
        let mut runner = Runner::new(core, shell);

        // Test button click
        runner
            .core()
            .send_event(MyEvent::ButtonClick(ButtonClick { x: 100, y: 200 }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        // Test key press
        runner.core().send_event(MyEvent::KeyPress(KeyPress {
            key: "Enter".to_string(),
        }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        // Test user login
        runner.core().send_event(MyEvent::UserLogin(UserLogin {
            username: "alice".to_string(),
        }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        // Test data received
        runner
            .core()
            .send_event(MyEvent::DataReceived(DataReceived {
                data: b"Test data from regular handler".to_vec(),
            }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        let model = runner.core().model();
        println!(
            "Regular handler - Clicks: {}, User: {:?}",
            model.click_count, model.username
        );
    }

    println!("\n=== Testing Same Handler Pattern ===");

    // Test with same magic handler pattern
    {
        let (core, shell) = Syzygy::builder()
            .model(MyModel::default())
            .update(handle_events_with_logging)
            .build();

        let shell = shell.with_effect_handler(handle_effects);
        let mut runner = Runner::new(core, shell);

        // Test data received
        runner
            .core()
            .send_event(MyEvent::DataReceived(DataReceived {
                data: b"Hello, derive macro!".to_vec(),
            }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        // Test error handling
        runner.core().send_event(MyEvent::Error(Error {
            message: "Test error".to_string(),
        }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        // Test key press with Enter
        runner.core().send_event(MyEvent::KeyPress(KeyPress {
            key: "Enter".to_string(),
        }))?;
        runner.tick(syzygy::spawn::spawner()).await?;

        let model = runner.core().model();
        println!("Second handler - Error: {:?}, Last key: {:?}", model.error_message, model.last_key);
    }

    println!("\n=== Derive Macro Benefits ===");
    println!("✅ Zero boilerplate: #[derive(MagicVariants)] generates everything");
    println!("✅ Clean syntax: Just derive and you're done");
    println!("✅ Type safety: Compile-time verification of variant structs");
    println!("✅ Optimal performance: No runtime overhead, compiles to jump tables");
    println!("✅ Grug-approved: Simple, debuggable, no ceremony");

    Ok(())
}
