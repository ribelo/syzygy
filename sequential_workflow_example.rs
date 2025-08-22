use serde::{Deserialize, Serialize};
use std::time::Duration;

// Domain types
#[derive(Debug, Clone)]
struct UserId(u32);

#[derive(Debug, Clone)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Debug, Clone)]
struct UserData {
    id: UserId,
    name: String,
    email: String,
}

#[derive(Debug, Clone)]
struct AddressData {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Clone)]
struct SandwichPrefs {
    bread: String,
    filling: String,
}

// Events - the unified pipeline
#[derive(Debug, Clone)]
enum AppEvent {
    // User-initiated
    StartLoginFlow { credentials: Credentials },
    
    // Login sequence events
    LoginSucceeded { user_id: UserId, user_data: UserData },
    LoginFailed { error: String },
    
    // User data sequence events  
    UserDataFetched { user_data: UserData },
    UserDataFetchFailed { error: String },
    
    // Address sequence events
    AddressDataFetched { address: AddressData },
    AddressDataFetchFailed { error: String },
    
    // Final sandwich event
    SandwichMade { message: String },
    SandwichFailed { error: String },
}

// Effects - the complex domain operations
#[derive(Debug, Clone)]
enum AppEffect {
    LoginUser { credentials: Credentials },
    FetchUserData { user_id: UserId },
    FetchAddressData { user_id: UserId },
    MakeASandwichForUser { user_id: UserId, preferences: SandwichPrefs },
    
    // The user's challenge: do we need composite effects?
    // Option 1: Composite effect (effect explosion)
    // LoginThenFetchThenAddressThenSandwich { credentials: Credentials },
    
    // Option 2: Sequential effect composition (what they want)
    // SequentialEffects { effects: Vec<AppEffect> },
}

// Model to track workflow state
#[derive(Debug, Default)]
struct AppModel {
    // Current workflow state
    current_user: Option<UserId>,
    user_data: Option<UserData>,
    address_data: Option<AddressData>,
    
    // Workflow tracking
    is_in_login_flow: bool,
    workflow_step: WorkflowStep,
    
    // Error state
    last_error: Option<String>,
}

#[derive(Debug, Default)]
enum WorkflowStep {
    #[default]
    Idle,
    LoggingIn,
    FetchingUserData,
    FetchingAddress,
    MakingSandwich,
    Complete,
    Failed,
}

// The App implementation using current Syzygy
struct SequentialWorkflowApp;

impl App for SequentialWorkflowApp {
    type Event = AppEvent;
    type Model = AppModel;
    type Effect = AppEffect;

    fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
        use AppEvent::*;
        use AppEffect::*;
        use WorkflowStep::*;

        match event {
            // Start the sequential workflow
            StartLoginFlow { credentials } => {
                model.is_in_login_flow = true;
                model.workflow_step = LoggingIn;
                model.last_error = None;
                
                Command::effect(LoginUser { credentials })
            }
            
            // Login succeeded - proceed to next step
            LoginSucceeded { user_id, user_data } => {
                if model.is_in_login_flow {
                    model.current_user = Some(user_id.clone());
                    model.user_data = Some(user_data);
                    model.workflow_step = FetchingUserData;
                    
                    Command::effect(FetchUserData { user_id })
                } else {
                    // Login succeeded but we're not in a workflow
                    model.current_user = Some(user_id);
                    Command::none()
                }
            }
            
            // Login failed - abort workflow
            LoginFailed { error } => {
                model.is_in_login_flow = false;
                model.workflow_step = Failed;
                model.last_error = Some(error);
                Command::none()
            }
            
            // User data fetched - proceed to address
            UserDataFetched { user_data } => {
                if model.is_in_login_flow && matches!(model.workflow_step, FetchingUserData) {
                    model.user_data = Some(user_data);
                    model.workflow_step = FetchingAddress;
                    
                    if let Some(user_id) = &model.current_user {
                        Command::effect(FetchAddressData { user_id: user_id.clone() })
                    } else {
                        // This shouldn't happen but handle gracefully
                        model.workflow_step = Failed;
                        model.last_error = Some("No user ID available".to_string());
                        Command::none()
                    }
                } else {
                    // Received user data but not in workflow
                    model.user_data = Some(user_data);
                    Command::none()
                }
            }
            
            // User data fetch failed - abort workflow
            UserDataFetchFailed { error } => {
                if model.is_in_login_flow {
                    model.is_in_login_flow = false;
                    model.workflow_step = Failed;
                    model.last_error = Some(error);
                }
                Command::none()
            }
            
            // Address data fetched - proceed to sandwich
            AddressDataFetched { address } => {
                if model.is_in_login_flow && matches!(model.workflow_step, FetchingAddress) {
                    model.address_data = Some(address);
                    model.workflow_step = MakingSandwich;
                    
                    if let Some(user_id) = &model.current_user {
                        let preferences = SandwichPrefs {
                            bread: "sourdough".to_string(),
                            filling: "turkey".to_string(),
                        };
                        Command::effect(MakeASandwichForUser { 
                            user_id: user_id.clone(), 
                            preferences 
                        })
                    } else {
                        model.workflow_step = Failed;
                        model.last_error = Some("No user ID available".to_string());
                        Command::none()
                    }
                } else {
                    // Received address data but not in workflow
                    model.address_data = Some(address);
                    Command::none()
                }
            }
            
            // Address fetch failed - abort workflow
            AddressDataFetchFailed { error } => {
                if model.is_in_login_flow {
                    model.is_in_login_flow = false;
                    model.workflow_step = Failed;
                    model.last_error = Some(error);
                }
                Command::none()
            }
            
            // Sandwich made - workflow complete!
            SandwichMade { message } => {
                if model.is_in_login_flow {
                    model.is_in_login_flow = false;
                    model.workflow_step = Complete;
                    println!("Workflow complete: {}", message);
                }
                Command::none()
            }
            
            // Sandwich failed - workflow failed
            SandwichFailed { error } => {
                if model.is_in_login_flow {
                    model.is_in_login_flow = false;
                    model.workflow_step = Failed;
                    model.last_error = Some(error);
                }
                Command::none()
            }
        }
    }
}

// Mock effect handler
use std::future::Future;
use std::pin::Pin;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

fn handle_effects(effect: AppEffect, ctx: EffectContext<AppEvent>) -> BoxFuture<()> {
    Box::pin(async move {
        use AppEffect::*;
        use AppEvent::*;
        
        match effect {
            LoginUser { credentials } => {
                // Simulate complex login logic
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                if credentials.username == "admin" && credentials.password == "password" {
                    let user_id = UserId(42);
                    let user_data = UserData {
                        id: user_id.clone(),
                        name: "Admin User".to_string(),
                        email: "admin@example.com".to_string(),
                    };
                    let _ = ctx.send_event(LoginSucceeded { user_id, user_data });
                } else {
                    let _ = ctx.send_event(LoginFailed { 
                        error: "Invalid credentials".to_string() 
                    });
                }
            }
            
            FetchUserData { user_id } => {
                // Simulate fetching user data
                tokio::time::sleep(Duration::from_millis(50)).await;
                
                let user_data = UserData {
                    id: user_id,
                    name: "John Doe".to_string(),
                    email: "john@example.com".to_string(),
                };
                let _ = ctx.send_event(UserDataFetched { user_data });
            }
            
            FetchAddressData { user_id: _ } => {
                // Simulate fetching address data
                tokio::time::sleep(Duration::from_millis(75)).await;
                
                let address = AddressData {
                    street: "123 Main St".to_string(),
                    city: "Anytown".to_string(),
                    zip: "12345".to_string(),
                };
                let _ = ctx.send_event(AddressDataFetched { address });
            }
            
            MakeASandwichForUser { user_id: _, preferences } => {
                // Simulate complex sandwich making
                tokio::time::sleep(Duration::from_millis(200)).await;
                
                let message = format!(
                    "Made a delicious {} sandwich with {} filling!", 
                    preferences.bread, 
                    preferences.filling
                );
                let _ = ctx.send_event(SandwichMade { message });
            }
        }
    })
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use syzygy::prelude::*;
    
    let (core, shell) = Syzygy::builder()
        .app(SequentialWorkflowApp)
        .model(AppModel::default())
        .build();

    let shell = shell.with_effect_handler(handle_effects);
    let mut runner = Runner::new(core, shell);

    // Start the sequential workflow
    let credentials = Credentials {
        username: "admin".to_string(),
        password: "password".to_string(),
    };
    
    runner.core().send_event(AppEvent::StartLoginFlow { credentials })?;
    
    // Run until workflow is complete
    runner.run_until(
        |_core, _shell| true, // Placeholder: adjust condition to your model state
        syzygy::spawn::spawner()
    ).await?;
    
    println!("Final model: {:#?}", runner.core().model());
    
    Ok(())
}
