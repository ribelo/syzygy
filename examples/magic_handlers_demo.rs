//! Magic Handlers Demonstration
//! 
//! This example showcases the magic handler system that enables Axum-style
//! parameter extraction for both update functions and effect handlers.
//! 
//! The magic handler system allows handlers to declare exactly what data
//! they need, and the system automatically extracts it from the appropriate
//! context (EventContext for updates, EffectContext for effects).

use syzygy::prelude::*;

// ============================================================================
// Application Models & Resources
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserModel {
    name: String,
    email: String,
    login_count: u32,
}

#[derive(Debug, Clone)]
struct DatabaseResource {
    connection_url: String,
    max_connections: u32,
}

// ============================================================================
// Events & Effects
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    UserLogin { email: String },
    UserLogout { email: String },
}

#[derive(Debug, Clone)]
enum AppEffect {
    SaveUserToDatabase { email: String, login_count: u32 },
    LogActivity { message: String, level: String },
}

// ============================================================================
// Custom Extractors
// ============================================================================

/// Extract the current login count for operations
struct LoginCount(u32);

impl<'a, Event, Effect, Storage> FromEventContext<'a, Event, Effect, Storage> for LoginCount
where
    Storage: syzygy::storage::Selector<UserModel, syzygy::storage::storage::Here>,
{
    fn from_context(ctx: &'a EventContext<Event, Effect, Storage>) -> Self {
        LoginCount(ctx.model::<UserModel, syzygy::storage::storage::Here>().login_count)
    }
}

/// Extract user name
struct UserName(String);

impl<'a, Event, Effect, Storage> FromEventContext<'a, Event, Effect, Storage> for UserName
where
    Storage: syzygy::storage::Selector<UserModel, syzygy::storage::storage::Here>,
{
    fn from_context(ctx: &'a EventContext<Event, Effect, Storage>) -> Self {
        UserName(ctx.model::<UserModel, syzygy::storage::storage::Here>().name.clone())
    }
}

/// Extract database connection URL
struct DatabaseUrl(String);

impl<Event, Resources> FromEffectContext<Event, Resources> for DatabaseUrl
where
    Resources: syzygy::storage::Selector<DatabaseResource, syzygy::storage::storage::Here>,
    Event: Send + 'static,
    Resources: Send + Sync + 'static,
{
    fn from_context(ctx: &EffectContext<Event, Resources>) -> Self {
        DatabaseUrl(ctx.resource::<DatabaseResource, syzygy::storage::storage::Here>().connection_url.clone())
    }
}

// ============================================================================
// Magic Event Handlers (Update Side)
// ============================================================================

/// Handle user login with automatic extraction of user data
fn handle_user_login(
    event: AppEvent,
    user_name: UserName,
    login_count: LoginCount,
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::UserLogin { email } = event {
        println!("🔐 User {} logging in (current user: {}, login count: {})", 
                email, user_name.0, login_count.0);
        
        // Return commands to save user and log activity
        Command::batch([
            Command::effect(AppEffect::SaveUserToDatabase { 
                email: email.clone(), 
                login_count: login_count.0 + 1 
            }),
            Command::effect(AppEffect::LogActivity {
                message: format!("User {} logged in", email),
                level: "INFO".to_string(),
            }),
        ])
    } else {
        Command::none()
    }
}

/// Simple handler with just event - no extraction
fn handle_user_logout(event: AppEvent) -> Command<AppEvent, AppEffect> {
    if let AppEvent::UserLogout { email } = event {
        println!("👋 User {} logging out", email);
        
        Command::effect(AppEffect::LogActivity {
            message: format!("User {} logged out", email),
            level: "INFO".to_string(),
        })
    } else {
        Command::none()
    }
}

// ============================================================================
// Magic Effect Handlers (Effect Side)
// ============================================================================

/// Handle database saves with automatic database URL extraction
async fn handle_database_save(
    effect: AppEffect,
    db_url: DatabaseUrl,
) {
    if let AppEffect::SaveUserToDatabase { email, login_count } = effect {
        println!("💾 Saving user {} (login #{}) to database at {}", 
                email, login_count, db_url.0);
        
        // Simulate async database operation
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        println!("✅ User {} saved successfully", email);
    }
}

/// Simple handler with no resource extraction
async fn handle_logging(effect: AppEffect) {
    if let AppEffect::LogActivity { message, level } = effect {
        println!("📝 [{}] {}", level, message);
        
        // Simulate async file writing
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }
}

// ============================================================================
// Main Demonstration
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Magic Handlers Demonstration");
    println!("==============================");
    println!("This example shows how magic handlers automatically extract parameters.");
    
    // Create a simple single-model storage for demonstration
    let mut user_storage = EmptyStorage.with_model(UserModel {
        name: "alice".to_string(),
        email: "alice@example.com".to_string(),
        login_count: 5,
    });
    
    // Create resources storage
    let resources = std::sync::Arc::new(EmptyStorage.with_model(DatabaseResource {
        connection_url: "postgres://localhost:5432/demo".to_string(),
        max_connections: 10,
    }));
    
    println!("\n📋 Testing Event Magic Handlers:");
    println!("================================");
    
    // Create event context
    let mut event_ctx = EventContext::<AppEvent, AppEffect, _>::new(&mut user_storage);
    
    // Test 1: User login with multiple extractions
    println!("\n1️⃣  Testing user login handler:");
    let login_event = AppEvent::UserLogin { email: "bob@example.com".to_string() };
    let login_command = handle_user_login.call_with_event(login_event, &mut event_ctx);
    
    // Collect and display effects
    let login_effects: Vec<_> = login_command.into_iter()
        .filter_map(|step| match step {
            CommandStep::Effect(effect) => Some(effect),
            _ => None,
        })
        .collect();
    
    println!("   Generated {} effects from login", login_effects.len());
    
    // Test 2: User logout (simple handler with no extraction)
    println!("\n2️⃣  Testing user logout handler:");
    let logout_event = AppEvent::UserLogout { 
        email: "bob@example.com".to_string(),
    };
    let logout_command = handle_user_logout.call_with_event(logout_event, &mut event_ctx);
    let logout_effects: Vec<_> = logout_command.into_iter()
        .filter_map(|step| match step {
            CommandStep::Effect(effect) => Some(effect),
            _ => None,
        })
        .collect();
    
    println!("   Generated {} effects from logout", logout_effects.len());
    
    println!("\n🔧 Testing Effect Magic Handlers:");
    println!("=================================");
    
    // Create effect context
    let effect_ctx = EffectContext::<AppEvent, _>::new(None, resources);
    
    // Test effect handlers with the generated effects
    println!("\n3️⃣  Processing login effects with magic handlers:");
    
    // Process each effect from the login command
    for (i, effect) in login_effects.iter().enumerate() {
        println!("\n   Processing effect {}: {:?}", i + 1, effect);
        
        match effect {
            AppEffect::SaveUserToDatabase { .. } => {
                handle_database_save.call_with_effect(effect.clone(), effect_ctx.clone()).await;
            },
            AppEffect::LogActivity { .. } => {
                handle_logging.call_with_effect(effect.clone(), effect_ctx.clone()).await;
            },
            _ => {}
        }
    }
    
    // Process logout effects
    println!("\n4️⃣  Processing logout effects:");
    for (i, effect) in logout_effects.iter().enumerate() {
        println!("\n   Processing effect {}: {:?}", i + 1, effect);
        
        match effect {
            AppEffect::LogActivity { .. } => {
                handle_logging.call_with_effect(effect.clone(), effect_ctx.clone()).await;
            },
            _ => {}
        }
    }
    
    println!("\n✨ Magic Handler Demonstration Complete!");
    println!("\n🎯 Key Features Demonstrated:");
    println!("   ✅ Automatic parameter extraction from EventContext");
    println!("   ✅ Automatic parameter extraction from EffectContext");
    println!("   ✅ Custom extractors for specific fields");
    println!("   ✅ Event handlers with and without extraction");
    println!("   ✅ Effect handlers with and without extraction");
    println!("   ✅ Clean, focused handler functions");
    println!("   ✅ Type-safe extraction with compile-time verification");
    
    println!("\n🔮 Ready for Phase 2 Integration:");
    println!("   - Builder API integration (.on_event<T>(), .on_effect<T>())");
    println!("   - Code generation for zero-overhead dispatch");
    println!("   - Runtime handler registration and metadata collection");
    
    Ok(())
}
