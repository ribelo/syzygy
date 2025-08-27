//! Example demonstrating Shell.resource<T>() API
//!
//! This example shows how to access resources from within the Shell,
//! demonstrating both read-only resources and resources with interior mutability.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use syzygy::event_context::EventContext;
use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum AppEvent {
    FetchUserData { user_id: u32 },
    CacheResult { key: String, value: String },
    UserDataFetched { user_id: u32, name: String },
    DataCached,
}

#[derive(Debug, Clone)]
enum AppEffect {
    HttpGet { url: String },
    CacheWrite { key: String, value: String },
    LogMessage { message: String },
}

#[derive(Debug, Default)]
struct AppModel {
    user_name: String,
    requests_count: u32,
}

// Read-only resource - no synchronization overhead
#[derive(Debug, Clone)]
struct HttpClient {
    base_url: String,
    timeout_seconds: u32,
}

// Resource with interior mutability
#[derive(Debug, Clone)]
struct Cache {
    data: Arc<Mutex<HashMap<String, String>>>,
}

impl HttpClient {
    fn new() -> Self {
        Self {
            base_url: "https://jsonplaceholder.typicode.com".to_string(),
            timeout_seconds: 30,
        }
    }

    fn get_user_url(&self, user_id: u32) -> String {
        format!("{}/users/{}", self.base_url, user_id)
    }
}

impl Cache {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn insert(&self, key: String, value: String) {
        self.data.lock().unwrap().insert(key, value);
    }

    fn get(&self, key: &str) -> Option<String> {
        self.data.lock().unwrap().get(key).cloned()
    }
}

// Update function
fn app_update(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, Storage<AppModel, EmptyStorage>>,
) -> Command<AppEvent, AppEffect> {
    let model: &mut AppModel = ctx.model_mut();

    match event {
        AppEvent::FetchUserData { user_id } => {
            model.requests_count += 1;
            Command::effect(AppEffect::HttpGet {
                url: format!("/users/{user_id}"),
            })
        }
        AppEvent::CacheResult { key, value } => {
            Command::effect(AppEffect::CacheWrite { key, value })
        }
        AppEvent::UserDataFetched { user_id, name } => {
            model.user_name = name.clone();

            // Cache the result and log
            Command::batch(vec![
                Command::effect(AppEffect::CacheWrite {
                    key: format!("user_{user_id}"),
                    value: name,
                }),
                Command::effect(AppEffect::LogMessage {
                    message: format!("User {user_id} data fetched"),
                }),
            ])
        }
        AppEvent::DataCached => Command::effect(AppEffect::LogMessage {
            message: "Data cached successfully".to_string(),
        }),
    }
}

// Effect handler demonstrating resource access
async fn handle_effects(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, Storage<Cache, Storage<HttpClient, EmptyStorage>>>,
) {
    match effect {
        AppEffect::HttpGet { url } => {
            // Access read-only HTTP client - no mutex overhead!
            let client: &HttpClient = ctx.resource();
            let full_url = client.get_user_url(1); // Simulate fetching user 1

            println!(
                "Making HTTP request to: {} (timeout: {}s)",
                full_url, client.timeout_seconds
            );

            // Simulate API response
            let user_name = "John Doe";
            let _ = ctx.send_event(AppEvent::UserDataFetched {
                user_id: 1,
                name: user_name.to_string(),
            });
        }
        AppEffect::CacheWrite { key, value } => {
            // Access cache with interior mutability
            let cache: &Cache = ctx.resource();
            cache.insert(key.clone(), value.clone());

            println!("Cached: {key} = {value}");
            let _ = ctx.send_event(AppEvent::DataCached);
        }
        AppEffect::LogMessage { message } => {
            println!("LOG: {message}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Shell Resource Access Example");
    println!("=============================");

    // Build the system with resources
    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .resource(HttpClient::new()) // Read-only resource
        .resource(Cache::new()) // Resource with interior mutability
        .update(app_update)
        .build();

    // Configure shell with effect handler
    let shell = shell.with_effect_handler(handle_effects);

    // Create runner
    let mut runner = Runner::new(core, shell);

    // Demonstrate Shell resource access
    println!("\nDemonstrating Shell.resource() access:");

    // Access resources from Shell directly
    let http_client: &HttpClient = runner.shell().resource();
    println!("HTTP Client base URL: {}", http_client.base_url);
    println!("HTTP Client timeout: {}s", http_client.timeout_seconds);

    let cache: &Cache = runner.shell().resource();
    cache.insert("demo_key".to_string(), "demo_value".to_string());
    println!("Cached demo data: {:?}", cache.get("demo_key"));

    // Send events to test the system
    runner
        .core()
        .send_event(AppEvent::FetchUserData { user_id: 1 })?;

    // Run event loop
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

    // Check final state
    let model: &AppModel = runner.core().model();
    println!("\nFinal state:");
    println!("User name: {}", model.user_name);
    println!("Requests made: {}", model.requests_count);

    // Show cached data
    let final_cache: &Cache = runner.shell().resource();
    println!("Cache contains user_1: {:?}", final_cache.get("user_1"));

    Ok(())
}
