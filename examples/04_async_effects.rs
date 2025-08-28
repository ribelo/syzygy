//! Example 04: Async Effects and Resources
//!
//! This example demonstrates advanced async effect handling with resources.
//! You'll learn:
//! - Resource management in EffectContext
//! - Async task spawning and management
//! - Event-driven async workflows
//! - Background task coordination

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use syzygy::prelude::*;

// ============================================================================
// Application State
// ============================================================================

#[derive(Debug, Clone, Default)]
struct AppModel {
    active_tasks: u32,
    completed_tasks: u32,
    messages: Vec<String>,
    last_result: Option<String>,
}

// ============================================================================
// Resources for Effect Handlers
// ============================================================================

#[derive(Debug, Clone)]
struct HttpClient {
    base_url: String,
    timeout_ms: u64,
}

impl HttpClient {
    fn new(base_url: String) -> Self {
        Self {
            base_url,
            timeout_ms: 5000,
        }
    }
    
    async fn get(&self, path: &str) -> Result<String, String> {
        println!("HTTP GET: {}{}", self.base_url, path);
        
        // Simulate HTTP request
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        // Simulate occasional failures
        if path.contains("error") {
            Err("HTTP 500: Server Error".to_string())
        } else {
            Ok(format!("Response from {}{}", self.base_url, path))
        }
    }
}

#[derive(Debug, Clone)]
struct DatabasePool {
    connections: Arc<AtomicU32>,
    max_connections: u32,
}

impl DatabasePool {
    fn new(max_connections: u32) -> Self {
        Self {
            connections: Arc::new(AtomicU32::new(0)),
            max_connections,
        }
    }
    
    async fn execute(&self, query: &str) -> Result<String, String> {
        let current = self.connections.load(Ordering::SeqCst);
        if current >= self.max_connections {
            return Err("Connection pool exhausted".to_string());
        }
        
        self.connections.fetch_add(1, Ordering::SeqCst);
        
        println!("DB EXEC: {} (connections: {})", query, current + 1);
        
        // Simulate database operation
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        
        self.connections.fetch_sub(1, Ordering::SeqCst);
        
        Ok(format!("Query result: {}", query))
    }
}

#[derive(Debug, Clone)]
struct CacheManager {
    cache: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl CacheManager {
    fn new() -> Self {
        Self {
            cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
    
    fn get(&self, key: &str) -> Option<String> {
        self.cache.lock().unwrap().get(key).cloned()
    }
    
    fn set(&self, key: String, value: String) {
        self.cache.lock().unwrap().insert(key, value);
    }
}

// ============================================================================
// Events & Effects
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    StartTask { task_id: String },
    TaskCompleted { task_id: String, result: String },
    TaskFailed { task_id: String, error: String },
    FetchData { url_path: String },
    SaveData { key: String, value: String },
    LoadCachedData { key: String },
    BatchProcess { items: Vec<String> },
}

#[derive(Debug, Clone)]
enum AppEffect {
    HttpRequest { task_id: String, path: String },
    DatabaseQuery { task_id: String, query: String },
    CacheOperation { operation: CacheOp },
    ParallelTasks { task_ids: Vec<String> },
    DelayedTask { task_id: String, delay_ms: u64 },
}

#[derive(Debug, Clone)]
enum CacheOp {
    Get { key: String },
    Set { key: String, value: String },
}

// Define our resource storage type
type ResourceStorage = Storage<CacheManager, Storage<DatabasePool, Storage<HttpClient, EmptyStorage>>>;

// ============================================================================
// Event Handlers
// ============================================================================

fn update_app(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, Storage<AppModel, EmptyStorage>>,
) -> Command<AppEvent, AppEffect> {
    let model: &mut AppModel = ctx.model_mut();
    
    match event {
        AppEvent::StartTask { task_id } => {
            model.active_tasks += 1;
            model.messages.push(format!("Started task: {}", task_id));
            
            // Decide what type of task to start
            if task_id.contains("http") {
                Command::effect(AppEffect::HttpRequest {
                    task_id,
                    path: "/api/data".to_string(),
                })
            } else if task_id.contains("db") {
                Command::effect(AppEffect::DatabaseQuery {
                    task_id,
                    query: "SELECT * FROM users".to_string(),
                })
            } else if task_id.contains("batch") {
                Command::effect(AppEffect::ParallelTasks {
                    task_ids: vec![
                        format!("{}_1", task_id),
                        format!("{}_2", task_id),
                        format!("{}_3", task_id),
                    ],
                })
            } else {
                Command::effect(AppEffect::DelayedTask {
                    task_id,
                    delay_ms: 200,
                })
            }
        }
        
        AppEvent::TaskCompleted { task_id, result } => {
            model.active_tasks = model.active_tasks.saturating_sub(1);
            model.completed_tasks += 1;
            model.last_result = Some(result.clone());
            model.messages.push(format!("Completed task: {} - {}", task_id, result));
            
            // Cache the result
            Command::effect(AppEffect::CacheOperation {
                operation: CacheOp::Set {
                    key: task_id,
                    value: result,
                },
            })
        }
        
        AppEvent::TaskFailed { task_id, error } => {
            model.active_tasks = model.active_tasks.saturating_sub(1);
            model.messages.push(format!("Failed task: {} - {}", task_id, error));
            
            Command::none()
        }
        
        AppEvent::FetchData { url_path } => {
            let task_id = format!("http_fetch_{}", model.completed_tasks);
            model.active_tasks += 1;
            
            Command::effect(AppEffect::HttpRequest {
                task_id,
                path: url_path,
            })
        }
        
        AppEvent::SaveData { key, value } => {
            Command::batch([
                Command::effect(AppEffect::CacheOperation {
                    operation: CacheOp::Set { key: key.clone(), value: value.clone() },
                }),
                Command::effect(AppEffect::DatabaseQuery {
                    task_id: format!("save_{}", key),
                    query: format!("INSERT INTO data (key, value) VALUES ('{}', '{}')", key, value),
                }),
            ])
        }
        
        AppEvent::LoadCachedData { key } => {
            Command::effect(AppEffect::CacheOperation {
                operation: CacheOp::Get { key },
            })
        }
        
        AppEvent::BatchProcess { items } => {
            let task_ids: Vec<String> = items
                .into_iter()
                .enumerate()
                .map(|(i, item)| format!("batch_{}_{}", i, item))
                .collect();
            
            model.active_tasks += task_ids.len() as u32;
            
            Command::effect(AppEffect::ParallelTasks { task_ids })
        }
    }
}

// ============================================================================
// Async Effect Handlers with Resource Extraction
// ============================================================================

async fn handle_http_request(
    effect: AppEffect,
    http_client: &HttpClient,
    sender: EventSender<AppEvent>,
) {
    if let AppEffect::HttpRequest { task_id, path } = effect {
        match http_client.get(&path).await {
            Ok(response) => {
                let _ = sender.send(AppEvent::TaskCompleted {
                    task_id,
                    result: response,
                });
            }
            Err(error) => {
                let _ = sender.send(AppEvent::TaskFailed { task_id, error });
            }
        }
    }
}

async fn handle_database_query(
    effect: AppEffect,
    db_pool: &DatabasePool,
    sender: EventSender<AppEvent>,
) {
    if let AppEffect::DatabaseQuery { task_id, query } = effect {
        match db_pool.execute(&query).await {
            Ok(result) => {
                let _ = sender.send(AppEvent::TaskCompleted { task_id, result });
            }
            Err(error) => {
                let _ = sender.send(AppEvent::TaskFailed { task_id, error });
            }
        }
    }
}

async fn handle_cache_operation(
    effect: AppEffect,
    cache: &CacheManager,
    sender: EventSender<AppEvent>,
) {
    if let AppEffect::CacheOperation { operation } = effect {
        match operation {
            CacheOp::Get { key } => {
                if let Some(value) = cache.get(&key) {
                    println!("CACHE HIT: {} -> {}", key, value);
                    let _ = sender.send(AppEvent::TaskCompleted {
                        task_id: format!("cache_get_{}", key),
                        result: value,
                    });
                } else {
                    println!("CACHE MISS: {}", key);
                }
            }
            CacheOp::Set { key, value } => {
                cache.set(key.clone(), value.clone());
                println!("CACHE SET: {} -> {}", key, value);
            }
        }
    }
}

async fn handle_parallel_tasks(
    effect: AppEffect,
    ctx: EffectContext<AppEvent, ResourceStorage>,
) {
    if let AppEffect::ParallelTasks { task_ids } = effect {
        println!("Starting {} parallel tasks", task_ids.len());
        
        for task_id in task_ids {
            let task_id_clone = task_id.clone();
            let ctx_clone = ctx.clone();
            let task = async move {
                // Simulate parallel work
                #[cfg(feature = "tokio")]
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;

                let result = format!("Parallel result for {}", task_id_clone);
                let _ = ctx_clone.send_event(AppEvent::TaskCompleted {
                    task_id: task_id_clone,
                    result,
                });
            };
            ctx.spawn(task).unwrap();
        }
    }
}

async fn handle_delayed_task(
    effect: AppEffect,
    sender: EventSender<AppEvent>,
) {
    if let AppEffect::DelayedTask { task_id, delay_ms } = effect {
        println!("Starting delayed task {} ({}ms)", task_id, delay_ms);
        
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        
        let _ = sender.send(AppEvent::TaskCompleted {
            task_id,
            result: "Delayed task completed".to_string(),
        });
    }
}

// ============================================================================
// Main Effect Dispatcher
// ============================================================================

async fn handle_effects(effect: AppEffect, ctx: EffectContext<AppEvent, ResourceStorage>) {
    let sender = EventSender(ctx.event_sender().unwrap());
    
    match &effect {
        AppEffect::HttpRequest { .. } => {
            let http_client: &HttpClient = ctx.resource();
            handle_http_request(effect, http_client, sender).await;
        }
        AppEffect::DatabaseQuery { .. } => {
            let db_pool: &DatabasePool = ctx.resource();
            handle_database_query(effect, db_pool, sender).await;
        }
        AppEffect::CacheOperation { .. } => {
            let cache: &CacheManager = ctx.resource();
            handle_cache_operation(effect, cache, sender).await;
        }
        AppEffect::ParallelTasks { .. } => {
            handle_parallel_tasks(effect, ctx).await;
        }
        AppEffect::DelayedTask { .. } => {
            handle_delayed_task(effect, sender).await;
        }
    }
}

// ============================================================================
// Main Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Async Effects and Resources Demo ===");
    println!("Demonstrating resource management and async task coordination\n");
    
    // Build system with resources
    let (core, shell) = Syzygy::builder()
        .model(AppModel::default())
        .resource(HttpClient::new("https://api.example.com".to_string()))
        .resource(DatabasePool::new(3))
        .resource(CacheManager::new())
        .update(update_app)
        .build();
    
    let shell = shell.with_effect_handler(handle_effects);
    let mut runner = Runner::new(core, shell);
    
    println!("Starting various async tasks:\n");
    
    // Test 1: HTTP request
    println!("1. Starting HTTP request task");
    runner.core().send_event(AppEvent::StartTask {
        task_id: "http_task_1".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Test 2: Database query
    println!("2. Starting database query task");
    runner.core().send_event(AppEvent::StartTask {
        task_id: "db_task_1".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Test 3: Batch parallel processing
    println!("3. Starting batch processing");
    runner.core().send_event(AppEvent::BatchProcess {
        items: vec!["item1".to_string(), "item2".to_string(), "item3".to_string()],
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Test 4: Cache operations
    println!("4. Testing cache operations");
    runner.core().send_event(AppEvent::SaveData {
        key: "user_123".to_string(),
        value: "user_data".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    runner.core().send_event(AppEvent::LoadCachedData {
        key: "user_123".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Test 5: Delayed task
    println!("5. Starting delayed task");
    runner.core().send_event(AppEvent::StartTask {
        task_id: "delayed_task_1".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Wait for all async operations to complete
    println!("\nWaiting for all tasks to complete...");
    for _ in 0..10 {
        runner.tick(syzygy::spawn::spawner()).await?;
        #[cfg(feature = "tokio")]
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let model: &AppModel = runner.core().model();
        if model.active_tasks == 0 {
            break;
        }
    }
    
    // Show final state
    let model: &AppModel = runner.core().model();
    println!("\nFinal State:");
    println!("  Active tasks: {}", model.active_tasks);
    println!("  Completed tasks: {}", model.completed_tasks);
    println!("  Last result: {:?}", model.last_result);
    println!("  Messages: {:?}", model.messages);
    
    println!("\nAsync Effects Key Points:");
    println!("✅ Resources provide shared services to effects");
    println!("✅ EffectContext enables safe task spawning");
    println!("✅ Event-driven async workflows");
    println!("✅ Background tasks automatically cancelled");
    println!("✅ Type-safe resource extraction");
    
    Ok(())
}
