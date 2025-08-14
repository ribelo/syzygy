//! Simple magic handler example
//!
//! Demonstrates basic magic handler functionality with field extraction

use syzygy::prelude::*;

#[derive(Debug, Default, Clone)]
struct UserStorage {
    users: Vec<String>,
    count: u32,
}

#[derive(Debug, Default, Clone)]
struct Analytics {
    events: u64,
}

// Simple model with magic extractors
#[derive(Debug, Default, ModelExtractors)]
struct AppModel {
    #[extract]
    storage: UserStorage,
    
    #[extract]
    analytics: Analytics,
    
    // This field won't be extractable
    cache: Vec<String>,
}

// Simple magic handler functions
fn get_user_count(storage: UserStorage) -> u32 {
    storage.count
}

fn get_analytics_info(analytics: Analytics, storage: UserStorage) -> String {
    format!("Events: {}, Users: {}", analytics.events, storage.count)
}

fn no_params_handler() -> String {
    "No parameters needed".to_string()
}

fn main() {
    println!("=== Simple Magic Handlers Example ===\n");

    let mut model = AppModel {
        storage: UserStorage {
            users: vec!["Alice".to_string(), "Bob".to_string()],
            count: 2,
        },
        analytics: Analytics {
            events: 42,
        },
        cache: vec!["cached_item".to_string()],
    };

    println!("Initial model: {:#?}\n", model);

    // Example 1: Handler with one parameter
    println!("1. Getting user count...");
    let count = MagicHandler::call(get_user_count, &mut model);
    println!("User count: {}\n", count);

    // Example 2: Handler with two parameters  
    println!("2. Getting analytics info...");
    let info = MagicHandler::call(get_analytics_info, &mut model);
    println!("Analytics: {}\n", info);

    // Example 3: Handler with no parameters
    println!("3. No parameters handler...");
    let result = MagicHandler::call(no_params_handler, &mut model);
    println!("Result: {}\n", result);

    println!("=== Simple Magic Handlers Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_extractors() {
        let model = AppModel::default();
        
        // Test that we can extract fields using FromModel
        let storage: UserStorage = FromModel::from_model(&model);
        assert_eq!(storage.count, 0);
        
        let analytics: Analytics = FromModel::from_model(&model);
        assert_eq!(analytics.events, 0);
    }

    #[test]
    fn test_simple_magic_handler() {
        let mut model = AppModel {
            storage: UserStorage { users: vec![], count: 5 },
            analytics: Analytics { events: 10 },
            cache: vec![],
        };

        let count = MagicHandler::call(get_user_count, &mut model);
        assert_eq!(count, 5);

        let info = MagicHandler::call(get_analytics_info, &mut model);
        assert!(info.contains("Events: 10"));
        assert!(info.contains("Users: 5"));
    }
}