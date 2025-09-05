//! Example 02: Multi-Model Storage with Magic Handlers
//!
//! This example demonstrates how to work with multiple models using magic handlers.
//! You'll learn:
//! - Creating storage chains with multiple model types using the storage_type! macro
//! - Automatic model extraction in magic handlers
//! - Model interactions and cross-model updates
//! - Type-safe model access patterns with parameter injection

use syzygy::prelude::*;
use syzygy::streaming::EffectOutput;
use syzygy::storage_type;

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

// Define our storage type using the storage_type! macro
// Before: Storage<SessionModel, Storage<AppConfigModel, Storage<UserModel, EmptyStorage>>>
// Now: Clean and readable!
type AppStorage = storage_type!(UserModel, AppConfigModel, SessionModel);

// ============================================================================
// Magic Event Handlers - Clean Multi-Model Access
// ============================================================================

/// Handle user login with automatic multi-model extraction
fn handle_user_login(
    event: AppEvent, 
    user: &mut UserModel, 
    session: &mut SessionModel, 
    config: &AppConfigModel
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::UserLogin { email } = event {
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
    } else {
        Command::none()
    }
}

/// Handle user logout - read user, modify session
fn handle_user_logout(
    event: AppEvent,
    user: &UserModel,
    session: &mut SessionModel
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::UserLogout = event {
        println!("User {} logging out from session {}", user.name, session.session_id);
        
        session.is_active = false;
        session.session_id.clear();
        
        Command::effect(AppEffect::LogActivity {
            message: format!("User {} logged out", user.name),
        })
    } else {
        Command::none()
    }
}

/// Handle theme change - modify config, read user
fn handle_theme_change(
    event: AppEvent,
    config: &mut AppConfigModel,
    user: &UserModel
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::ChangeTheme { theme } = event {
        let old_theme = config.theme.clone();
        config.theme = theme.clone();
        
        println!("User {} changed theme from {} to {}", user.name, old_theme, theme);
        
        Command::batch([
            Command::effect(AppEffect::SaveUserPreferences),
            Command::effect(AppEffect::RefreshUI),
        ])
    } else {
        Command::none()
    }
}

/// Handle language update - modify config, read user
fn handle_language_update(
    event: AppEvent,
    config: &mut AppConfigModel,
    user: &UserModel
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::UpdateLanguage { language } = event {
        config.language = language.clone();
        
        println!("User {} changed language to {}", user.name, language);
        
        Command::effect(AppEffect::SaveUserPreferences)
    } else {
        Command::none()
    }
}

/// Handle session expiration - modify session, read user
fn handle_session_expired(
    event: AppEvent,
    session: &mut SessionModel,
    user: &UserModel
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::SessionExpired = event {
        println!("Session {} expired for user {}", session.session_id, user.name);
        
        session.is_active = false;
        
        // Chain events - session expiry triggers logout
        Command::event(AppEvent::UserLogout)
    } else {
        Command::none()
    }
}

/// Handle activity detection - only session needed
fn handle_activity_detected(
    event: AppEvent,
    session: &mut SessionModel
) -> Command<AppEvent, AppEffect> {
    if let AppEvent::ActivityDetected = event {
        if session.is_active {
            session.last_activity = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            println!("Activity detected, session updated: {}", session.session_id);
        }
        
        Command::none()
    } else {
        Command::none()
    }
}

// ============================================================================
// Main Update Function - Dispatches to Magic Handlers
// ============================================================================

fn update_app(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppStorage>,
) -> Command<AppEvent, AppEffect> {
    // Dispatch to appropriate magic handler based on event type
    match event {
        AppEvent::UserLogin { .. } => event_trigger(event, ctx, handle_user_login),
        AppEvent::UserLogout => event_trigger(event, ctx, handle_user_logout),
        AppEvent::ChangeTheme { .. } => event_trigger(event, ctx, handle_theme_change),
        AppEvent::UpdateLanguage { .. } => event_trigger(event, ctx, handle_language_update),
        AppEvent::SessionExpired => event_trigger(event, ctx, handle_session_expired),
        AppEvent::ActivityDetected => event_trigger(event, ctx, handle_activity_detected),
    }
}

// ============================================================================
// Magic Effect Handlers
// ============================================================================

/// Simple effect-only handler
fn handle_save_preferences(effect: AppEffect) {
    if let AppEffect::SaveUserPreferences = effect {
        println!("EFFECT: Saving user preferences to database...");
    }
}

/// Effect-only handler for logging
fn handle_log_activity(effect: AppEffect) {
    if let AppEffect::LogActivity { message } = effect {
        println!("EFFECT: Activity log - {}", message);
    }
}

/// Effect-only handler for UI refresh
fn handle_refresh_ui(effect: AppEffect) {
    if let AppEffect::RefreshUI = effect {
        println!("EFFECT: Refreshing user interface...");
    }
}

/// Main effect dispatcher using magic handlers
async fn handle_effects(effect: AppEffect, _ctx: EffectContext<AppEvent, EmptyStorage>) -> EffectOutput<AppEvent> {
    match effect {
        AppEffect::SaveUserPreferences => handle_save_preferences(effect),
        AppEffect::LogActivity { .. } => handle_log_activity(effect),
        AppEffect::RefreshUI => handle_refresh_ui(effect),
    }
    EffectOutput::None
}

// ============================================================================
// Main Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Model Storage with Magic Handlers Demo ===");
    println!("Demonstrating automatic parameter extraction across multiple models\n");
    
    // Build storage chain with multiple models
    let (core, shell) = Syzygy::builder()
        .model(UserModel::default())
        .model(AppConfigModel {
            theme: "light".to_string(),
            language: "en".to_string(),
        })
        .model(SessionModel::default())
        .event_handler(update_app)
        .effect_handler(handle_effects)
        .build();
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
    
    println!("Magic Handlers Multi-Model Key Points:");
    println!("✅ Automatic parameter extraction - no manual ctx.model() calls");
    println!("✅ Mix read-only (&T) and mutable (&mut T) access automatically");
    println!("✅ Type-safe dependency injection at compile time");
    println!("✅ Clean, focused handlers - each only declares what it needs");
    println!("✅ Storage chains support multiple model types seamlessly");
    println!("✅ Zero runtime overhead - compiles to direct function calls");
    
    Ok(())
}
