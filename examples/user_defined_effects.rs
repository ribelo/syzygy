use std::time::Duration;
use std::collections::HashMap;
use syzygy::prelude::*;
use syzygy::event_context::EventContext;

/// User-defined events for a simple todo app
#[derive(Debug, Clone)]
enum TodoEvent {
    AddTodo { text: String },
    TodoAdded { id: u32, text: String },
    RemoveTodo { id: u32 },
    TodoRemoved { id: u32 },
    LoadTodos,
    TodosLoaded { todos: Vec<Todo> },
    SaveTodos,
    TodosSaved,
    Error { message: String },
}

/// User-defined effects - simple unidirectional approach
/// Users define their own effects based on their needs
#[derive(Debug, Clone)]
enum TodoEffect {
    /// HTTP request to API
    HttpRequest { 
        method: String,
        url: String, 
        headers: HashMap<String, String>,
        body: Option<String>,
    },
    /// Timer delay
    Timer { duration: Duration },
    /// Log message  
    Log { level: LogLevel, message: String },
    /// Save to database
    Database { 
        query: String,
        params: Vec<String>,
    },
}

/// Simple data structures
#[derive(Debug, Clone)]
struct Todo {
    id: u32,
    text: String,
    completed: bool,
}

#[derive(Debug, Clone)]
enum LogLevel {
    Info,
    Debug,
    Warn,
    Error,
}

/// Application model
#[derive(Debug)]
struct TodoModel {
    todos: Vec<Todo>,
    next_id: u32,
    is_loading: bool,
    error_message: Option<String>,
}

impl Default for TodoModel {
    fn default() -> Self {
        Self {
            todos: Vec::new(),
            next_id: 1,
            is_loading: false,
            error_message: None,
        }
    }
}

use syzygy::storage::{Storage, EmptyStorage};

fn todo_update(
    event: TodoEvent,
    ctx: &mut EventContext<TodoEvent, TodoEffect, Storage<TodoModel, EmptyStorage>>,
) -> Command<TodoEvent, TodoEffect> {
    let model: &mut TodoModel = ctx.model_mut();
        match event {
            TodoEvent::AddTodo { text } => {
                if text.trim().is_empty() {
                    return Command::batch([
                        Command::event(TodoEvent::Error {
                            message: "Todo text cannot be empty".to_string(),
                        }),
                        Command::effect(TodoEffect::Log {
                            level: LogLevel::Warn,
                            message: "Attempted to add empty todo".to_string(),
                        }),
                    ]);
                }

                let id = model.next_id;
                model.next_id += 1;
                
                // Add todo locally first
                model.todos.push(Todo {
                    id,
                    text: text.clone(),
                    completed: false,
                });

                // Then sync to server
                Command::batch([
                    Command::event(TodoEvent::TodoAdded { id, text: text.clone() }),
                    Command::effect(TodoEffect::Log {
                        level: LogLevel::Debug,
                        message: format!("Adding todo {id} to local storage"),
                    }),
                    Command::effect(TodoEffect::HttpRequest {
                        method: "POST".to_string(),
                        url: "/api/todos".to_string(),
                        headers: HashMap::from([
                            ("Content-Type".to_string(), "application/json".to_string())
                        ]),
                        body: Some(format!(r#"{{"id": {id}, "text": "{text}"}}"#)),
                    }),
                ])
            }

            TodoEvent::TodoAdded { id, text } => {
                Command::effect(TodoEffect::Log {
                    level: LogLevel::Info,
                    message: format!("Todo added: {id} - {text}"),
                })
            }

            TodoEvent::LoadTodos => {
                model.is_loading = true;
                Command::effect(TodoEffect::HttpRequest {
                    method: "GET".to_string(),
                    url: "/api/todos".to_string(),
                    headers: HashMap::new(),
                    body: None,
                })
            }

            TodoEvent::TodosLoaded { todos } => {
                model.todos = todos;
                model.is_loading = false;
                Command::effect(TodoEffect::Log {
                    level: LogLevel::Info,
                    message: format!("Loaded {} todos", model.todos.len()),
                })
            }

            TodoEvent::SaveTodos => {
                Command::effect(TodoEffect::Database {
                    query: "INSERT INTO todos (id, text, completed) VALUES (?, ?, ?)".to_string(),
                    params: model.todos.iter()
                        .flat_map(|todo| vec![
                            todo.id.to_string(),
                            todo.text.clone(),
                            todo.completed.to_string(),
                        ])
                        .collect(),
                })
            }

            TodoEvent::TodosSaved => {
                Command::effect(TodoEffect::Log {
                    level: LogLevel::Info,
                    message: "Todos saved to database".to_string(),
                })
            }

            TodoEvent::Error { message } => {
                model.error_message = Some(message.clone());
                Command::effect(TodoEffect::Log {
                    level: LogLevel::Error,
                    message,
                })
            }

            TodoEvent::RemoveTodo { id } => {
                model.todos.retain(|todo| todo.id != id);
                Command::batch([
                    Command::event(TodoEvent::TodoRemoved { id }),
                    Command::effect(TodoEffect::HttpRequest {
                        method: "DELETE".to_string(),
                        url: format!("/api/todos/{id}"),
                        headers: HashMap::new(),
                        body: None,
                    }),
                ])
            }

            TodoEvent::TodoRemoved { id } => {
                Command::effect(TodoEffect::Log {
                    level: LogLevel::Info,
                    message: format!("Todo removed: {id}"),
                })
            }
        }
}

/// AFIT Effect handler - converts effects to async operations with zero-cost abstractions
/// This is where users implement their own I/O logic using function pointers (no captures)
async fn handle_effect(
    effect: TodoEffect,
    ctx: EffectContext<TodoEvent, EmptyStorage>,
) {
    // No more boxing overhead! Pure AFIT implementation
        match effect {
            TodoEffect::HttpRequest { method, url, headers, body } => {
                println!("🌐 HTTP {method} {url}");
                println!("   Headers: {headers:?}");
                if let Some(body) = body {
                    println!("   Body: {body}");
                }
                
                // Simulate HTTP request with timer delay
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                #[cfg(feature = "smol")]
                smol::Timer::after(Duration::from_millis(100)).await;
                
                #[cfg(feature = "async-std")]
                async_std::task::sleep(Duration::from_millis(100)).await;
                
                // Send response back as event
                let response_event = if url.contains("/api/todos") && method == "GET" {
                    TodoEvent::TodosLoaded {
                        todos: vec![
                            Todo { id: 1, text: "Example todo".to_string(), completed: false },
                        ],
                    }
                } else {
                    TodoEvent::Error {
                        message: format!("HTTP {method} {url} completed"),
                    }
                };
                let _ = ctx.send_event(response_event);
            }

            TodoEffect::Timer { duration } => {
                println!("⏰ Timer delay: {duration:?}");
                #[cfg(feature = "tokio")]
                tokio::time::sleep(duration).await;
                
                #[cfg(feature = "smol")]
                smol::Timer::after(duration).await;
                
                #[cfg(feature = "async-std")]
                async_std::task::sleep(duration).await;
            }

            TodoEffect::Log { level, message } => {
                let level_str = match level {
                    LogLevel::Info => "INFO",
                    LogLevel::Debug => "DEBUG", 
                    LogLevel::Warn => "WARN",
                    LogLevel::Error => "ERROR",
                };
                println!("📝 [{level_str}] {message}");
            }

            TodoEffect::Database { query, params } => {
                println!("🗄️  Database query: {query}");
                println!("   Params: {params:?}");
                
                // Simulate database operation
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_millis(50)).await;
                
                #[cfg(feature = "smol")]
                smol::Timer::after(Duration::from_millis(50)).await;
                
                #[cfg(feature = "async-std")]
                async_std::task::sleep(Duration::from_millis(50)).await;
                
                let _ = ctx.send_event(TodoEvent::TodosSaved);
            }
        }
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Syzygy Todo App with User-Defined Effects");
    
    // Build the system (auto-wired by default)
    let (core, shell) = Syzygy::builder::<TodoEvent, TodoEffect>()
        .model(TodoModel::default())
        .update(todo_update)
        .build();
    
    // Set up the effect handler
    let event_sender = core.event_sender();
    let shell = shell.with_effect_handler(handle_effect);
    
    // Use Runner for automatic orchestration
    let mut runner = Runner::new(core, shell);
    
    // Send some events to demonstrate the system
    event_sender.send(TodoEvent::AddTodo { 
        text: "Learn Syzygy".to_string() 
    })?;
    
    event_sender.send(TodoEvent::LoadTodos)?;
    
    event_sender.send(TodoEvent::SaveTodos)?;
    
    // Demo removing a todo
    event_sender.send(TodoEvent::RemoveTodo { id: 1 })?;
    
    // Run for a bit to process all events using zero-cost spawner
    runner.run_until(
        |core, _shell| !core.model().todos.is_empty() && !core.model().is_loading,
        syzygy::spawn::spawner() // Zero-cost RPIT spawner!
    ).await?;
    
    println!("\n✅ Final state:");
    println!("   Todos: {:?}", runner.core().model().todos);
    println!("   Error: {:?}", runner.core().model().error_message);
    
    Ok(())
}

#[cfg(all(feature = "smol", not(feature = "tokio")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        println!("🚀 Starting Syzygy Todo App with User-Defined Effects (smol runtime)");
        
        // Build the system (auto-wired by default)
        let (core, shell) = Syzygy::builder::<TodoEvent, TodoEffect>()
            .model(TodoModel::default())
            .update(todo_update)
            .build();
        
        // Set up the effect handler
        let event_sender = core.event_sender();
        let shell = shell.with_effect_handler(handle_effect);
        
        // Use Runner for automatic orchestration
        let mut runner = Runner::new(core, shell);
        
        // Send some events to demonstrate the system
        event_sender.send(TodoEvent::AddTodo { 
            text: "Learn Syzygy".to_string() 
        })?;
        
        event_sender.send(TodoEvent::LoadTodos)?;
        event_sender.send(TodoEvent::SaveTodos)?;
        event_sender.send(TodoEvent::RemoveTodo { id: 1 })?;
        
        // Run for a bit to process all events using zero-cost spawner
        runner.run_until(
            |core, _shell| core.model().todos.len() > 0 && !core.model().is_loading,
            syzygy::spawn::spawner() // Zero-cost RPIT spawner!
        ).await?;
        
        println!("\n✅ Final state:");
        println!("   Todos: {:?}", runner.core().model().todos);
        println!("   Error: {:?}", runner.core().model().error_message);
        
        Ok(())
    })
}

#[cfg(all(feature = "async-std", not(feature = "tokio"), not(feature = "smol")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    async_std::task::block_on(async {
        println!("🚀 Starting Syzygy Todo App with User-Defined Effects (async-std runtime)");
        
        // Build the system (auto-wired by default)
        let (core, shell) = Syzygy::builder::<TodoEvent, TodoEffect>()
            .model(TodoModel::default())
            .update(todo_update)
            .build();
        
        // Set up the effect handler
        let event_sender = core.event_sender();
        let shell = shell.with_effect_handler(handle_effect);
        
        // Use Runner for automatic orchestration
        let mut runner = Runner::new(core, shell);
        
        // Send some events to demonstrate the system
        event_sender.send(TodoEvent::AddTodo { 
            text: "Learn Syzygy".to_string() 
        })?;
        
        event_sender.send(TodoEvent::LoadTodos)?;
        event_sender.send(TodoEvent::SaveTodos)?;
        event_sender.send(TodoEvent::RemoveTodo { id: 1 })?;
        
        // Run for a bit to process all events using zero-cost spawner
        runner.run_until(
            |core, _shell| core.model().todos.len() > 0 && !core.model().is_loading,
            syzygy::spawn::spawner() // Zero-cost RPIT spawner!
        ).await?;
        
        println!("\n✅ Final state:");
        println!("   Todos: {:?}", runner.core().model().todos);
        println!("   Error: {:?}", runner.core().model().error_message);
        
        Ok(())
    })
}

#[cfg(not(any(feature = "tokio", feature = "smol", feature = "async-std")))]
fn main() {
    panic!("Please enable one of the async runtime features: tokio, smol, or async-std");
}
