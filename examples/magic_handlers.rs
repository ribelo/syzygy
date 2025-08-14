//! Example demonstrating Axum-style magic handlers with automatic field extraction
//!
//! This example shows how to use the ModelExtractors derive macro and magic handlers
//! to create type-safe, ergonomic event handlers that automatically extract
//! the exact model fields they need.

use syzygy::prelude::*;

// Define separate model types
#[derive(Debug, Clone)]
struct User {
    id: u32,
    name: String,
    email: String,
}

#[derive(Debug, Default)]
struct UserStorage {
    users: Vec<User>,
    next_id: u32,
}

#[derive(Debug, Default)]
struct Analytics {
    user_count: u64,
    events_processed: u64,
}

#[derive(Debug, Default)]
struct Config {
    max_users: u32,
    email_notifications: bool,
}

// Main application model with magic extractors
#[derive(Debug, Default, ModelExtractors)]
struct AppModel {
    #[extract]
    storage: UserStorage,
    
    #[extract]
    analytics: Analytics,
    
    #[extract]
    config: Config,
    
    // Multiple User fields require disambiguation
    #[extract(as = "CurrentUser")]
    current_user: Option<User>,
    
    #[extract(as = "AdminUser")]
    admin_user: Option<User>,
    
    // This field won't be extractable
    cache: Vec<String>,
}

// Events and Commands
#[derive(Debug, Clone)]
enum AppEvent {
    CreateUser { name: String, email: String },
    UserCreated { user: User },
    LoginUser { user_id: u32 },
    UserLoggedIn { user: User },
    DeleteUser { user_id: u32 },
    UserDeleted { user_id: u32 },
    AnalyticsRequested,
}

#[derive(Debug, Clone)]
enum AppCommand {
    SendWelcomeEmail { user: User },
    LogEvent { event: String },
    BackupUsers,
}

// Traditional event handler (for comparison)
fn traditional_handler(event: AppEvent, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    match event {
        AppEvent::CreateUser { name, email } => {
            // Need to manually access nested fields
            if model.storage.users.len() >= model.config.max_users as usize {
                return Dispatch::none();
            }
            
            let user = User {
                id: model.storage.next_id,
                name,
                email,
            };
            
            model.storage.next_id += 1;
            model.storage.users.push(user.clone());
            model.analytics.user_count += 1;
            model.analytics.events_processed += 1;
            
            let mut dispatch = Dispatch::event(AppEvent::UserCreated { user: user.clone() });
            
            if model.config.email_notifications {
                dispatch = dispatch.with_command(AppCommand::SendWelcomeEmail { user });
            }
            
            dispatch
        }
        _ => Dispatch::none(),
    }
}

// Magic handlers - automatically extract only what they need!

fn create_user_handler(
    storage: &UserStorage,
    analytics: &Analytics, 
    config: &Config
) -> String {
    format!(
        "Create user handler called - storage has {} users, analytics shows {} events, max users: {}",
        storage.users.len(),
        analytics.events_processed,
        config.max_users
    )
}

fn login_user_handler(
    storage: &UserStorage,
    CurrentUser(current): CurrentUser
) -> String {
    format!(
        "Login handler - storage has {} users, current user: {:?}",
        storage.users.len(),
        current.as_ref().map(|u| &u.name)
    )
}

fn get_analytics_handler(
    analytics: &Analytics,
    storage: &UserStorage
) -> String {
    format!(
        "Analytics: {} users, {} events processed", 
        analytics.user_count, 
        analytics.events_processed
    )
}

fn admin_operation_handler(
    AdminUser(admin): AdminUser,
    storage: &UserStorage
) -> String {
    match admin {
        Some(admin_user) => {
            format!("Admin {} can backup {} users", admin_user.name, storage.users.len())
        }
        None => {
            "No admin user - access denied".to_string()
        }
    }
}

// Demonstration of magic handler usage
fn main() {
    println!("=== Magic Handlers Example ===\n");

    // Create initial model
    let mut model = AppModel {
        config: Config {
            max_users: 100,
            email_notifications: true,
        },
        admin_user: Some(User {
            id: 0,
            name: "Admin".to_string(),
            email: "admin@example.com".to_string(),
        }),
        ..Default::default()
    };

    println!("Initial model: {:#?}\n", model);

    // Example 1: Create user using magic handler
    println!("1. Create user handler with magic extraction...");
    let result = create_user_handler.call(&mut model);
    println!("Result: {}\n", result);

    // Example 2: Login user
    println!("2. Login user handler...");
    let result = login_user_handler.call(&mut model);
    println!("Result: {}\n", result);

    // Example 3: Get analytics (no parameters needed)
    println!("3. Getting analytics...");
    let result = get_analytics_handler.call(&mut model);
    println!("Result: {}\n", result);

    // Example 4: Admin operation
    println!("4. Admin operation handler...");
    let result = admin_operation_handler.call(&mut model);
    println!("Result: {}\n", result);

    // Example 5: Using magic handler extension trait
    println!("5. Using magic handler extension trait...");
    let result = get_analytics_handler.call_magic(&mut model);
    println!("Result: {}\n", result);

    println!("=== Magic Handlers Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_extractors() {
        let mut model = AppModel::default();
        
        // Test that we can extract fields using magic handlers
        let storage: &mut UserStorage = FromModelMut::from_model_mut(&mut model);
        storage.next_id = 42;
        
        let analytics: &Analytics = FromModel::from_model(&model);
        assert_eq!(analytics.user_count, 0);
        
        let config: &Config = FromModel::from_model(&model);
        assert_eq!(config.max_users, 0);
    }

    #[test]
    fn test_wrapper_types() {
        let mut model = AppModel {
            current_user: Some(User {
                id: 1,
                name: "Test".to_string(),
                email: "test@example.com".to_string(),
            }),
            admin_user: Some(User {
                id: 2,
                name: "Admin".to_string(),
                email: "admin@example.com".to_string(),
            }),
            ..Default::default()
        };

        // Test CurrentUser wrapper
        let CurrentUser(current): CurrentUser = FromModel::from_model(&model);
        assert!(current.is_some());
        assert_eq!(current.as_ref().unwrap().id, 1);

        // Test AdminUser wrapper
        let AdminUser(admin): AdminUser = FromModel::from_model(&model);
        assert!(admin.is_some());
        assert_eq!(admin.as_ref().unwrap().id, 2);
    }

    #[test]
    fn test_create_user_magic_handler() {
        let mut model = AppModel {
            config: Config {
                max_users: 2,
                email_notifications: true,
            },
            ..Default::default()
        };

        // Use magic handler to create user
        let result = create_user_handler.call(&mut model);

        // Verify the result contains expected information
        assert!(result.contains("0 users"));
        assert!(result.contains("max users: 2"));
    }

    #[test]
    fn test_login_magic_handler() {
        let mut model = AppModel::default();
        
        // Add a user first
        model.storage.users.push(User {
            id: 5,
            name: "Charlie".to_string(),
            email: "charlie@example.com".to_string(),
        });

        // Use magic handler to login
        let result = login_user_handler.call(&mut model);

        // Verify result contains expected information
        assert!(result.contains("1 users"));
        assert!(result.contains("None")); // No current user set
    }
}