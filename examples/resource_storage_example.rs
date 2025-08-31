//! Example demonstrating the new resource Storage API
//!
//! This example shows how to use multiple resources with the Storage pattern,
//! similar to how models work in Core.

use syzygy::event_context::EventContext;
use syzygy::prelude::*;
use syzygy::streaming::EffectOutput;

#[derive(Debug, Clone)]
enum AppEvent {
    FetchData,
    SaveConfig { theme: String },
    DataFetched { data: String },
    ConfigSaved,
}

#[derive(Debug, Clone)]
enum AppEffect {
    HttpGet { url: String },
    WriteFile { path: String, content: String },
}

#[derive(Debug, Default)]
struct AppModel {
    data: String,
    theme: String,
}

// Resource types
#[derive(Debug, Clone)]
struct HttpClient {
    base_url: String,
}

#[derive(Debug, Clone)]
struct FileSystem {
    base_path: String,
}

impl HttpClient {
    fn new() -> Self {
        Self {
            base_url: "https://api.example.com".to_string(),
        }
    }
}

impl FileSystem {
    fn new() -> Self {
        Self {
            base_path: "/tmp".to_string(),
        }
    }
}

// Update function using the model storage
fn app_update(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, Storage<AppModel, EmptyStorage>>,
) -> Command<AppEvent, AppEffect> {
    let model: &mut AppModel = ctx.model_mut();

    match event {
        AppEvent::FetchData => Command::effect(AppEffect::HttpGet {
            url: "/data".to_string(),
        }),
        AppEvent::SaveConfig { theme } => {
            model.theme.clone_from(&theme);
            Command::effect(AppEffect::WriteFile {
                path: "config.json".to_string(),
                content: format!(r#"{{"theme": "{theme}"}}"#),
            })
        }
        AppEvent::DataFetched { data } => {
            model.data = data.to_string();
            Command::none()
        }
        AppEvent::ConfigSaved => {
            println!("Config saved successfully");
            Command::none()
        }
    }
}

// Effect handler using the resource storage
async fn handle_effects(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, Storage<FileSystem, Storage<HttpClient, EmptyStorage>>>,
) -> EffectOutput<AppEvent> {
    match effect {
        AppEffect::HttpGet { url } => {
            // Access HttpClient resource from storage
            let client: &HttpClient = ctx.resource();
            let full_url = format!("{}{}", client.base_url, url);

            println!("Fetching data from: {full_url}");

            // Simulate HTTP request
            let data = format!("Data from {full_url}");
            EffectOutput::Single(AppEvent::DataFetched { data })
        }
        AppEffect::WriteFile { path, content } => {
            // Access FileSystem resource from storage
            let fs: &FileSystem = ctx.resource();
            let full_path = format!("{}/{}", fs.base_path, path);

            println!("Writing to file: {full_path}");
            println!("Content: {content}");

            // Simulate file write
            EffectOutput::Single(AppEvent::ConfigSaved)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Resource Storage API Example");
    println!("============================");

    // Build the system using the new resource API
    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .resource(HttpClient::new()) // Add HttpClient resource
        .resource(FileSystem::new()) // Add FileSystem resource
        .event_handler(app_update)
        .effect_handler(handle_effects)
        .build();

    // Effect handler provided via builder

    // Create runner
    let mut runner = Runner::new(core, shell);

    // Demonstrate Shell.resource() API
    println!("Shell resource access:");
    let http_client: &HttpClient = runner.shell().resource();
    println!("HTTP client base URL: {}", http_client.base_url);

    let filesystem: &FileSystem = runner.shell().resource();
    println!("Filesystem base path: {}", filesystem.base_path);
    println!();

    // Send some events to test the system
    runner.core().send_event(AppEvent::FetchData)?;
    runner.core().send_event(AppEvent::SaveConfig {
        theme: "dark".to_string(),
    })?;

    // Run a few ticks to process the events
    println!("\nProcessing events...\n");

    for i in 0..5 {
        let did_work = runner.tick(syzygy::spawn::spawner()).await?;
        if did_work {
            println!("Tick {}: processed work", i + 1);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        } else {
            break;
        }
    }

    // Check final model state using both APIs
    let model_via_storage: &AppModel = runner.core().storage().get();
    let model_via_direct: &AppModel = runner.core().model();

    println!("\nFinal model state (via storage API):");
    println!("Data: {}", model_via_storage.data);
    println!("Theme: {}", model_via_storage.theme);

    println!("\nFinal model state (via new model API):");
    println!("Data: {}", model_via_direct.data);
    println!("Theme: {}", model_via_direct.theme);

    // Verify both APIs return the same data
    assert_eq!(model_via_storage.data, model_via_direct.data);
    assert_eq!(model_via_storage.theme, model_via_direct.theme);

    Ok(())
}
