//! Example showing proper model chain usage in Syzygy

use syzygy::prelude::*;

// Define separate model structs
#[derive(Debug, Default)]
struct UserModel {
    users: Vec<String>,
    next_id: u32,
}

#[derive(Debug, Default)]
struct CounterModel {
    count: i32,
}

#[derive(Debug, Default)]
struct LogModel {
    entries: Vec<String>,
}

// Define a simple resource
#[derive(Debug, Default)]
struct Metrics {
    event_count: u64,
}

impl Metrics {
    fn increment(&self) {
        println!("Metrics: Event processed (total events incremented)");
    }
}

// Define your event enum
#[derive(Debug, Clone)]
enum AppEvent {
    CreateUser { name: String },
    IncrementCounter,
    DecrementCounter { amount: i32 },
    LogMessage { message: String },
}

// Define your task enum
#[derive(Debug, Clone)]
enum AppTask {
    SaveToDatabase { data: String },
    SendNotification { message: String },
}

// Implement the Task trait for async execution
impl<E, R> Task<E, R> for AppTask
where
    E: Clone + Send,
    R: Send,
{
    async fn execute(&self, _ctx: TaskContext<E, R>) {
        match self {
            AppTask::SaveToDatabase { data } => {
                println!("Task: Saving to database: {}", data);
            }
            AppTask::SendNotification { message } => {
                println!("Task: Sending notification: {}", message);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Model Chain Syzygy Example");
    println!("==============================");

    // Build the Syzygy system with multiple models
    let (mut syzygy, handle) = Syzygy::builder()
        .model(UserModel::default())          // First model
        .model(CounterModel::default())       // Second model  
        .model(LogModel::default())           // Third model
        .resource(Metrics::default())         // Metrics resource
        .event_handler(|event, models| {
            match event {
                AppEvent::CreateUser { name } => {
                    // Access the UserModel specifically
                    let user_model: &mut UserModel = models.get_mut();
                    user_model.users.push(name.clone());
                    user_model.next_id += 1;
                    
                    // Also log it
                    let log_model: &mut LogModel = models.get_mut();
                    log_model.entries.push(format!("User '{}' created", name));
                    
                    Dispatch::new(
                        vec![AppEvent::LogMessage { 
                            message: format!("Welcome {}", name) 
                        }],
                        vec![AppTask::SaveToDatabase { 
                            data: format!("user:{}", name) 
                        }]
                    )
                }
                AppEvent::IncrementCounter => {
                    // Access the CounterModel specifically
                    let counter: &mut CounterModel = models.get_mut();
                    counter.count += 1;
                    
                    Dispatch::tasks_only(vec![
                        AppTask::SendNotification { 
                            message: format!("Counter now: {}", counter.count) 
                        }
                    ])
                }
                AppEvent::DecrementCounter { amount } => {
                    let counter: &mut CounterModel = models.get_mut();
                    counter.count -= amount;
                    
                    Dispatch::empty()
                }
                AppEvent::LogMessage { message } => {
                    let log_model: &mut LogModel = models.get_mut();
                    log_model.entries.push(message.clone());
                    
                    Dispatch::empty()
                }
            }
        })
        .build();

    println!("\n📝 Dispatching events...");
    
    // Send some events
    handle.dispatch(AppEvent::CreateUser { name: "Alice".to_string() })?;
    handle.dispatch(AppEvent::IncrementCounter)?;
    handle.dispatch(AppEvent::CreateUser { name: "Bob".to_string() })?;
    handle.dispatch(AppEvent::DecrementCounter { amount: 3 })?;

    // Process all events
    syzygy.process_events();
    
    // Check final state using type-safe model access
    println!("\n📊 Final state:");
    let user_model: &UserModel = syzygy.model();
    let counter_model: &CounterModel = syzygy.model();
    let log_model: &LogModel = syzygy.model();
    
    println!("Users: {:?}", user_model.users);
    println!("Next ID: {}", user_model.next_id);
    println!("Counter: {}", counter_model.count);
    println!("Log entries: {:?}", log_model.entries);
    
    println!("\n✅ Model chain example completed!");
    
    Ok(())
}