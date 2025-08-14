//! Example showing proper resource chain usage in Syzygy

use syzygy::prelude::*;
use syzygy::resource::ResourceAccess;
use syzygy::chain::{Selector, Here};
use std::sync::Arc;

// Define model structs
#[derive(Debug, Default)]
struct UserModel {
    users: Vec<String>,
    next_id: u32,
}

// Define resource structs
#[derive(Debug)]
struct Database {
    connection_string: String,
}

impl Database {
    fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
    
    fn save(&self, data: &str) {
        println!("Database: Saving '{}' to {}", data, self.connection_string);
    }
}

#[derive(Debug)]
struct EmailService {
    smtp_host: String,
}

impl EmailService {
    fn new(smtp_host: String) -> Self {
        Self { smtp_host }
    }
    
    fn send_email(&self, to: &str, message: &str) {
        println!("Email: Sending '{}' to {} via {}", message, to, self.smtp_host);
    }
}

#[derive(Debug)]
struct Config {
    app_name: String,
    debug: bool,
}

impl Config {
    fn new(app_name: String, debug: bool) -> Self {
        Self { app_name, debug }
    }
}

// Define your event enum
#[derive(Debug, Clone)]
enum AppEvent {
    CreateUser { name: String, email: String },
    SendWelcomeEmail { email: String },
}

// Define your task enum
#[derive(Debug, Clone)]
enum AppTask {
    SaveUser { name: String, email: String },
    SendEmail { to: String, message: String },
    LogAction { action: String },
}

// Implement the Task trait for async execution with resource access
impl<E, R> Task<E, R> for AppTask
where
    E: Clone + Send,
    R: Send,
    TaskContext<E, R>: ResourceAccess<R>,
    R: Selector<Arc<Database>, Here>,
{
    async fn execute(&self, ctx: TaskContext<E, R>) {
        match self {
            AppTask::SaveUser { name, email } => {
                // Access the database resource
                let db: Arc<Database> = ctx.resource();
                db.save(&format!("user:{}:{}", name, email));
            }
            AppTask::SendEmail { to, message } => {
                // This task would need EmailService resource
                println!("Task: Would send email to {}: {}", to, message);
            }
            AppTask::LogAction { action } => {
                println!("Task: Action logged: {}", action);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Resource Chain Syzygy Example");
    println!("=================================");

    // Create resources
    let database = Database::new("postgresql://localhost/myapp".to_string());
    let email_service = EmailService::new("smtp.example.com".to_string());
    let config = Config::new("MyApp".to_string(), true);

    // Build the Syzygy system with multiple resources
    let (mut syzygy, handle) = Syzygy::builder()
        .model(UserModel::default())     // Single model
        .resource(database)              // First resource
        .resource(email_service)         // Second resource  
        .resource(config)                // Third resource
        .event_handler(|event, models| {
            match event {
                AppEvent::CreateUser { name, email } => {
                    // Access the UserModel
                    let user_model: &mut UserModel = models.get_mut();
                    user_model.users.push(name.clone());
                    user_model.next_id += 1;
                    
                    Dispatch::new(
                        vec![AppEvent::SendWelcomeEmail { email: email.clone() }],
                        vec![AppTask::SaveUser { 
                            name: name.clone(), 
                            email: email.clone() 
                        }]
                    )
                }
                AppEvent::SendWelcomeEmail { email } => {
                    Dispatch::tasks_only(vec![
                        AppTask::SendEmail { 
                            to: email.clone(), 
                            message: "Welcome to our service!".to_string() 
                        },
                        AppTask::LogAction { 
                            action: format!("Welcome email sent to {}", email) 
                        }
                    ])
                }
            }
        })
        .build();

    println!("\n📝 Dispatching events...");
    
    // Send some events
    handle.dispatch(AppEvent::CreateUser { 
        name: "Alice".to_string(), 
        email: "alice@example.com".to_string() 
    })?;
    handle.dispatch(AppEvent::CreateUser { 
        name: "Bob".to_string(), 
        email: "bob@example.com".to_string() 
    })?;

    // Process all events
    syzygy.process_events();
    
    // Check final state using type-safe model access
    println!("\n📊 Final state:");
    let user_model: &UserModel = syzygy.model();
    println!("Users: {:?}", user_model.users);
    println!("Next ID: {}", user_model.next_id);
    
    // Access resources directly from syzygy if needed
    println!("\n🔧 Resource information:");
    let config: &Arc<Config> = syzygy.resources().get();
    println!("App: {} (Debug: {})", config.app_name, config.debug);
    
    println!("\n✅ Resource chain example completed!");
    
    Ok(())
}