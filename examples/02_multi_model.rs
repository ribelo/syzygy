//! Example 02: Multi-Model Storage
//!
//! This example demonstrates how to work with multiple models in storage chains.
//! You'll learn:
//! - Creating storage chains with multiple model types
//! - Extracting specific models from context
//! - Model interactions and cross-model updates
//! - Type-safe model access patterns

use syzygy::prelude::*;

// ============================================================================
// Multiple Model Types
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserModel {
    name: String,
    email: String,
    login_count: u32,
}

#[derive(Debug, Clone, Default)]
struct AppConfigModel {
    theme: String,
    language: String,
}

#[derive(Debug, Clone, Default)]
struct SessionModel {
    session_id: String,
    is_active: bool,
    last_activity: u64,
}

// ============================================================================
// Events
// ============================================================================

#[derive(Debug, Clone)]
enum AppEvent {
    UserLogin { email: String },
    UserLogout,
    ChangeTheme { theme: String },
    UpdateLanguage { language: String },
    SessionExpired,
    ActivityDetected,
}

// ============================================================================
// Effects
// ============================================================================

#[derive(Debug, Clone)]
enum AppEffect {
    SaveUserPreferences,
    LogActivity { message: String },
    RefreshUI,
}

// Define our storage type
type AppStorage = Storage<SessionModel, Storage<AppConfigModel, Storage<UserModel, EmptyStorage>>>;

// ============================================================================
// Update Function with Multi-Model Access
// ============================================================================

fn update_app(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppStorage>,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogin { email } => {
            // Access and modify multiple models
            let user: &mut UserModel = ctx.model_mut();
            let session: &mut SessionModel = ctx.model_mut();
            let config: &AppConfigModel = ctx.model(); // Read-only access
            
            // Update user model
            user.email = email.clone();
            user.name = email.split('@').next().unwrap_or("user").to_string();
            user.login_count += 1;
            
            // Update session model
            session.session_id = format!("session_{}", user.login_count);
            session.is_active = true;
            session.last_activity = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            println!(
                "User {} logged in (count: {}), session: {}, theme: {}",
                user.name, user.login_count, session.session_id, config.theme
            );
            
            Command::batch([
                Command::effect(AppEffect::LogActivity {
                    message: format!("User {} logged in", user.name),
                }),
                Command::effect(AppEffect::RefreshUI),
            ])
        }
        
        AppEvent::UserLogout => {
            let user: &UserModel = ctx.model();
            let session: &mut SessionModel = ctx.model_mut();
            
            println!("User {} logging out from session {}", user.name, session.session_id);
            
            session.is_active = false;
            session.session_id.clear();
            
            Command::effect(AppEffect::LogActivity {
                message: format!("User {} logged out", user.name),
            })
        }
        
        AppEvent::ChangeTheme { theme } => {
            let config: &mut AppConfigModel = ctx.model_mut();
            let user: &UserModel = ctx.model();
            
            let old_theme = config.theme.clone();
            config.theme = theme.clone();
            
            println!("User {} changed theme from {} to {}", user.name, old_theme, theme);
            
            Command::batch([
                Command::effect(AppEffect::SaveUserPreferences),
                Command::effect(AppEffect::RefreshUI),
            ])
        }
        
        AppEvent::UpdateLanguage { language } => {
            let config: &mut AppConfigModel = ctx.model_mut();
            let user: &UserModel = ctx.model();
            
            config.language = language.clone();
            
            println!("User {} changed language to {}", user.name, language);
            
            Command::effect(AppEffect::SaveUserPreferences)
        }
        
        AppEvent::SessionExpired => {
            let session: &mut SessionModel = ctx.model_mut();
            let user: &UserModel = ctx.model();
            
            println!("Session {} expired for user {}", session.session_id, user.name);
            
            session.is_active = false;
            
            // Chain events - session expiry triggers logout
            Command::event(AppEvent::UserLogout)
        }
        
        AppEvent::ActivityDetected => {
            let session: &mut SessionModel = ctx.model_mut();
            
            if session.is_active {
                session.last_activity = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                println!("Activity detected, session updated: {}", session.session_id);
            }
            
            Command::none()
        }
    }
}

// ============================================================================
// Effect Handler
// ============================================================================

async fn handle_effects(effect: AppEffect, _ctx: EffectContext<AppEvent, EmptyStorage>) {
    match effect {
        AppEffect::SaveUserPreferences => {
            println!("EFFECT: Saving user preferences to database...");
        }
        AppEffect::LogActivity { message } => {
            println!("EFFECT: Activity log - {}", message);
        }
        AppEffect::RefreshUI => {
            println!("EFFECT: Refreshing user interface...");
        }
    }
}

// ============================================================================
// Main Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Model Storage Demo ===");
    println!("Demonstrating storage chains with multiple model types\n");
    
    // Build storage chain with multiple models
    let (core, shell) = Syzygy::builder()
        .model(UserModel::default())
        .model(AppConfigModel {
            theme: "light".to_string(),
            language: "en".to_string(),
        })
        .model(SessionModel::default())
        .update(update_app)
        .build();
    
    let shell = shell.with_effect_handler(handle_effects);
    let mut runner = Runner::new(core, shell);
    
    // Print initial state
    println!("Initial state:");
    let user: &UserModel = runner.core().model();
    let config: &AppConfigModel = runner.core().model();
    let session: &SessionModel = runner.core().model();
    println!("  User: {:?}", user);
    println!("  Config: {:?}", config);
    println!("  Session: {:?}\n", session);
    
    // Test user login
    println!("1. User login");
    runner.core().send_event(AppEvent::UserLogin {
        email: "alice@example.com".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Show updated state
    let user: &UserModel = runner.core().model();
    let session: &SessionModel = runner.core().model();
    println!("  Updated User: {:?}", user);
    println!("  Updated Session: {:?}\n", session);
    
    // Test theme change
    println!("2. Change theme");
    runner.core().send_event(AppEvent::ChangeTheme {
        theme: "dark".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    let config: &AppConfigModel = runner.core().model();
    println!("  Updated Config: {:?}\n", config);
    
    // Test activity detection
    println!("3. Activity detection");
    runner.core().send_event(AppEvent::ActivityDetected)?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    let session: &SessionModel = runner.core().model();
    println!("  Updated Session: {:?}\n", session);
    
    // Test 4: Language update
    println!("4. Testing language update...");
    runner.core().send_event(AppEvent::UpdateLanguage {
        language: "fr".to_string(),
    })?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Test session expiry (triggers logout)
    println!("5. Session expiry (chains to logout)");
    runner.core().send_event(AppEvent::SessionExpired)?;
    runner.tick(syzygy::spawn::spawner()).await?;
    
    let session: &SessionModel = runner.core().model();
    println!("  Final Session: {:?}\n", session);
    
    println!("Multi-Model Key Points:");
    println!("✅ Type-safe model access with ctx.model::<T>()");
    println!("✅ Storage chains allow multiple model types");
    println!("✅ Mix read-only and mutable access as needed");
    println!("✅ Models can interact through shared events");
    println!("✅ Zero-cost abstractions - compiles to direct access");
    
    Ok(())
}
