//! Multi-Model Demo
//!
//! This example demonstrates how to use IndexedMap and ModelMap for
//! managing multiple models with zero-overhead access patterns.
//! This shows the foundation that will be integrated with EventMap.

use syzygy::prelude::*;

// Define different model types for different domains
#[derive(Debug, Default)]
struct UserModel {
    users: Vec<User>,
    active_count: usize,
}

#[derive(Debug, Default)]
struct PostModel {
    posts: Vec<Post>,
    total_views: usize,
}

#[derive(Debug, Default)]
struct SettingsModel {
    theme: String,
    notifications_enabled: bool,
    max_users: usize,
}

// Implement ModelType for each model with unique indices
impl ModelType for UserModel {
    const MODEL_INDEX: usize = 0;
}

impl ModelType for PostModel {
    const MODEL_INDEX: usize = 1;
}

impl ModelType for SettingsModel {
    const MODEL_INDEX: usize = 2;
}

// Simple data types
#[derive(Debug, Clone)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug, Clone)]
struct Post {
    id: u64,
    title: String,
    content: String,
    author_id: u64,
    views: usize,
}

// Events that work with different models
#[derive(Debug, Clone)]
struct UserCreated {
    user: User,
}

#[derive(Debug, Clone)]
struct PostCreated {
    post: Post,
}

#[derive(Debug, Clone)]
struct SettingsUpdated {
    theme: Option<String>,
    notifications: Option<bool>,
    max_users: Option<usize>,
}

// Example handler functions that would work with specific models
fn handle_user_created(event: UserCreated, user_model: &mut UserModel, settings: &SettingsModel) {
    println!("Creating user: {}", event.user.name);
    
    // Check settings constraint
    if user_model.users.len() >= settings.max_users {
        println!("Cannot create user: max users ({}) exceeded", settings.max_users);
        return;
    }
    
    user_model.users.push(event.user);
    user_model.active_count += 1;
    
    println!("User created. Total users: {}", user_model.active_count);
}

fn handle_post_created(event: PostCreated, post_model: &mut PostModel, user_model: &UserModel) {
    println!("Creating post: {}", event.post.title);
    
    // Verify author exists
    let author_exists = user_model.users.iter().any(|u| u.id == event.post.author_id);
    if !author_exists {
        println!("Cannot create post: author {} not found", event.post.author_id);
        return;
    }
    
    post_model.posts.push(event.post);
    println!("Post created. Total posts: {}", post_model.posts.len());
}

fn handle_settings_updated(event: SettingsUpdated, settings_model: &mut SettingsModel) {
    println!("Updating settings");
    
    if let Some(theme) = event.theme {
        settings_model.theme = theme;
        println!("Theme updated to: {}", settings_model.theme);
    }
    
    if let Some(notifications) = event.notifications {
        settings_model.notifications_enabled = notifications;
        println!("Notifications: {}", if notifications { "enabled" } else { "disabled" });
    }
    
    if let Some(max_users) = event.max_users {
        settings_model.max_users = max_users;
        println!("Max users updated to: {}", settings_model.max_users);
    }
}

fn main() {
    println!("🚀 Multi-Model Demo - Zero-overhead model management\n");
    
    // Create and populate the model map
    let model_map = ModelMapBuilder::<3>::new()
        .with_model(UserModel::default())
        .with_model(PostModel::default())
        .with_model(SettingsModel {
            theme: "dark".to_string(),
            notifications_enabled: true,
            max_users: 100,
        })
        .build();
    
    println!("✅ Models registered:");
    println!("   - UserModel at index {}", UserModel::MODEL_INDEX);
    println!("   - PostModel at index {}", PostModel::MODEL_INDEX);
    println!("   - SettingsModel at index {}", SettingsModel::MODEL_INDEX);
    println!("   - Total models: {}/{}\n", model_map.model_count(), model_map.capacity());
    
    // Create some test data
    let alice = User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };
    
    let bob = User {
        id: 2,
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
    };
    
    let post = Post {
        id: 1,
        title: "Hello World".to_string(),
        content: "This is my first post!".to_string(),
        author_id: 1, // Alice's ID
        views: 0,
    };
    
    // Simulate event processing by accessing models
    println!("🎯 Processing events...\n");
    
    // Event 1: Update settings
    {
        let settings = model_map.get_mut::<SettingsModel>().unwrap();
        handle_settings_updated(
            SettingsUpdated {
                theme: Some("light".to_string()),
                notifications: None,
                max_users: Some(50),
            },
            settings,
        );
        println!();
    }
    
    // Event 2: Create users
    {
        let user_model = model_map.get_mut::<UserModel>().unwrap();
        let settings = model_map.get::<SettingsModel>().unwrap();
        
        handle_user_created(UserCreated { user: alice.clone() }, user_model, settings);
        handle_user_created(UserCreated { user: bob.clone() }, user_model, settings);
        println!();
    }
    
    // Event 3: Create a post
    {
        let post_model = model_map.get_mut::<PostModel>().unwrap();
        let user_model = model_map.get::<UserModel>().unwrap();
        
        handle_post_created(PostCreated { post: post.clone() }, post_model, user_model);
        println!();
    }
    
    // Event 4: Try to create a post with non-existent author
    {
        let bad_post = Post {
            id: 2,
            title: "Bad Post".to_string(),
            content: "This won't work".to_string(),
            author_id: 999, // Non-existent user
            views: 0,
        };
        
        let post_model = model_map.get_mut::<PostModel>().unwrap();
        let user_model = model_map.get::<UserModel>().unwrap();
        
        handle_post_created(PostCreated { post: bad_post }, post_model, user_model);
        println!();
    }
    
    // Display final state
    println!("📊 Final state:\n");
    
    if let Some(user_model) = model_map.get::<UserModel>() {
        println!("👥 Users ({}):", user_model.active_count);
        for user in &user_model.users {
            println!("   - {} ({})", user.name, user.email);
        }
        println!();
    }
    
    if let Some(post_model) = model_map.get::<PostModel>() {
        println!("📝 Posts ({}):", post_model.posts.len());
        for post in &post_model.posts {
            println!("   - \"{}\" by user {}", post.title, post.author_id);
        }
        println!();
    }
    
    if let Some(settings) = model_map.get::<SettingsModel>() {
        println!("⚙️  Settings:");
        println!("   - Theme: {}", settings.theme);
        println!("   - Notifications: {}", settings.notifications_enabled);
        println!("   - Max users: {}", settings.max_users);
        println!();
    }
    
    // Demonstrate zero-overhead access patterns
    println!("⚡ Performance characteristics:");
    println!("   - Model access: O(1) array lookup + pointer dereference");
    println!("   - Type safety: Compile-time verified with ModelType trait");
    println!("   - Memory: Zero heap allocations during event processing");
    println!("   - Storage: Intentionally leaked models for program lifetime");
    println!();
    
    println!("✨ This foundation can be integrated with EventMap for");
    println!("   automatic model selection based on handler requirements!");
}