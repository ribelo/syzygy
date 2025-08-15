//! EventMap correctness tests for the simplified API
//!
//! This test suite focuses on validating the EventMap implementation's
//! correctness with the simplified API that uses automatic variant indexing.

use syzygy::dispatch::Dispatch;
use syzygy::event_map::{EventMap, EventMapBuilder, EventVariant};
use syzygy_macros::Event;

// Test data types with various patterns
#[derive(Clone, Debug, PartialEq)]
struct UserCreated { id: u64, name: String }

#[derive(Clone, Debug, PartialEq)]
struct UserUpdated { id: u64, name: String, email: Option<String> }

#[derive(Clone, Debug, PartialEq)]
struct UserDeleted { id: u64 }

#[derive(Clone, Debug, PartialEq)]
struct PostCreated { id: u64, user_id: u64, title: String }

#[derive(Clone, Debug, PartialEq)]
struct PostUpdated { id: u64, title: String }

// Test event enum using Event derive macro
#[derive(Clone, Debug, Event)]
enum AppEvent {
    UserCreated(UserCreated),
    UserUpdated(UserUpdated),
    UserDeleted(UserDeleted),
    PostCreated(PostCreated),
    PostUpdated(PostUpdated),
}

// Test command enum
#[derive(Debug, Clone, PartialEq)]
enum AppCommand {
    SendEmail { to: String, subject: String },
    LogAction { action: String },
    UpdateIndex { entity_id: u64 },
    NotifySubscribers { post_id: u64 },
}

// Test model with realistic state
#[derive(Default, Debug, Clone, PartialEq)]
struct AppModel {
    users: std::collections::HashMap<u64, String>,
    posts: std::collections::HashMap<u64, String>,
    user_emails: std::collections::HashMap<u64, String>,
    event_count: u64,
    last_action: Option<String>,
    errors: Vec<String>,
}

// Handler functions with complex dispatch logic
fn handle_user_created(data: UserCreated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    model.event_count += 1;
    model.users.insert(data.id, data.name.clone());
    model.last_action = Some(format!("Created user: {}", data.name));
    
    Dispatch::new(
        vec![], // No events generated
        vec![
            AppCommand::LogAction { action: format!("User {} created", data.name) },
            AppCommand::SendEmail { 
                to: format!("{}@example.com", data.name.to_lowercase()),
                subject: "Welcome!".to_string()
            }
        ]
    )
}

fn handle_user_updated(data: UserUpdated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    model.event_count += 1;
    
    if let Some(existing_name) = model.users.get_mut(&data.id) {
        *existing_name = data.name.clone();
        
        // Update email if provided
        if let Some(email) = data.email {
            model.user_emails.insert(data.id, email);
        }
        
        model.last_action = Some(format!("Updated user: {}", data.name));
        
        Dispatch::command(AppCommand::LogAction { 
            action: format!("User {} updated", data.name) 
        })
    } else {
        model.errors.push(format!("Attempted to update non-existent user {}", data.id));
        Dispatch::none()
    }
}

fn handle_user_deleted(data: UserDeleted, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    model.event_count += 1;
    
    if let Some(name) = model.users.remove(&data.id) {
        model.user_emails.remove(&data.id);
        model.last_action = Some(format!("Deleted user: {}", name));
        
        // Generate events for cascade deletion of posts
        let user_posts: Vec<_> = model.posts.iter()
            .filter_map(|(post_id, title)| {
                // Simulate user_id extraction from title (simplified)
                if title.contains(&format!("by_user_{}", data.id)) {
                    Some(*post_id)
                } else {
                    None
                }
            })
            .collect();
        
        // Remove user's posts and generate deletion events
        let mut events = Vec::new();
        for post_id in user_posts {
            model.posts.remove(&post_id);
            // Note: Can't generate PostDeleted events as we don't have that variant
            // This is intentional to test that not all cascades need events
        }
        
        Dispatch::new(
            events,
            vec![
                AppCommand::LogAction { action: format!("User {} deleted", name) },
                AppCommand::UpdateIndex { entity_id: data.id }
            ]
        )
    } else {
        model.errors.push(format!("Attempted to delete non-existent user {}", data.id));
        Dispatch::none()
    }
}

fn handle_post_created(data: PostCreated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    model.event_count += 1;
    
    // Check if user exists
    if model.users.contains_key(&data.user_id) {
        model.posts.insert(data.id, format!("{}_by_user_{}", data.title, data.user_id));
        model.last_action = Some(format!("Created post: {}", data.title));
        
        Dispatch::new(
            vec![], // No cascade events
            vec![
                AppCommand::LogAction { action: format!("Post '{}' created", data.title) },
                AppCommand::NotifySubscribers { post_id: data.id },
                AppCommand::UpdateIndex { entity_id: data.id }
            ]
        )
    } else {
        model.errors.push(format!("Cannot create post for non-existent user {}", data.user_id));
        Dispatch::none()
    }
}

fn handle_post_updated(data: PostUpdated, model: &mut AppModel) -> Dispatch<AppEvent, AppCommand> {
    model.event_count += 1;
    
    if let Some(existing_title) = model.posts.get_mut(&data.id) {
        // Extract user_id from existing title
        let user_id_part = existing_title.split("_by_user_").nth(1);
        *existing_title = format!("{}_by_user_{}", data.title, user_id_part.unwrap_or("unknown"));
        
        model.last_action = Some(format!("Updated post: {}", data.title));
        
        Dispatch::command(AppCommand::LogAction { 
            action: format!("Post '{}' updated", data.title) 
        })
    } else {
        model.errors.push(format!("Attempted to update non-existent post {}", data.id));
        Dispatch::none()
    }
}

#[test]
fn test_simplified_api_basic_functionality() {
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_created)
        .on(handle_user_updated)
        .on(handle_user_deleted)
        .on(handle_post_created)
        .on(handle_post_updated)
        .build();
    
    let mut model = AppModel::default();
    
    // Test user creation
    let create_event = AppEvent::UserCreated(UserCreated { 
        id: 1, 
        name: "Alice".to_string() 
    });
    
    let result = event_map.dispatch(create_event, &mut model);
    
    assert_eq!(model.users.get(&1).unwrap(), "Alice");
    assert_eq!(model.event_count, 1);
    assert_eq!(result.commands.len(), 2);
    assert!(matches!(result.commands[0], AppCommand::LogAction { .. }));
    assert!(matches!(result.commands[1], AppCommand::SendEmail { .. }));
}

#[test]
fn test_automatic_variant_indexing() {
    // Verify that EventVariant trait is properly implemented
    assert_eq!(UserCreated::VARIANT_INDEX, 0);
    assert_eq!(UserUpdated::VARIANT_INDEX, 1);
    assert_eq!(UserDeleted::VARIANT_INDEX, 2);
    assert_eq!(PostCreated::VARIANT_INDEX, 3);
    assert_eq!(PostUpdated::VARIANT_INDEX, 4);
}

#[test]
fn test_complex_dispatch_scenario() {
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_created)
        .on(handle_user_updated)
        .on(handle_user_deleted)
        .on(handle_post_created)
        .on(handle_post_updated)
        .build();
    
    let mut model = AppModel::default();
    
    // Create user
    event_map.dispatch(AppEvent::UserCreated(UserCreated { 
        id: 1, 
        name: "Alice".to_string() 
    }), &mut model);
    
    // Create post
    let post_result = event_map.dispatch(AppEvent::PostCreated(PostCreated { 
        id: 100, 
        user_id: 1, 
        title: "Hello World".to_string() 
    }), &mut model);
    
    assert_eq!(model.users.len(), 1);
    assert_eq!(model.posts.len(), 1);
    assert_eq!(model.event_count, 2);
    assert_eq!(post_result.commands.len(), 3); // Log, Notify, UpdateIndex
    
    // Update user with email
    event_map.dispatch(AppEvent::UserUpdated(UserUpdated { 
        id: 1, 
        name: "Alice Smith".to_string(),
        email: Some("alice@example.com".to_string())
    }), &mut model);
    
    assert_eq!(model.users.get(&1).unwrap(), "Alice Smith");
    assert_eq!(model.user_emails.get(&1).unwrap(), "alice@example.com");
    
    // Delete user (should also affect posts)
    let delete_result = event_map.dispatch(AppEvent::UserDeleted(UserDeleted { id: 1 }), &mut model);
    
    assert_eq!(model.users.len(), 0);
    assert_eq!(model.posts.len(), 0); // Post should be removed
    assert_eq!(delete_result.commands.len(), 2); // Log, UpdateIndex
}

#[test]
fn test_error_handling() {
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_updated)
        .on(handle_user_deleted)
        .on(handle_post_created)
        .on(handle_post_updated)
        .build();
    
    let mut model = AppModel::default();
    
    // Try to update non-existent user
    let result = event_map.dispatch(AppEvent::UserUpdated(UserUpdated { 
        id: 999, 
        name: "Ghost".to_string(),
        email: None
    }), &mut model);
    
    assert!(result.commands.is_empty());
    assert!(result.events.is_empty());
    assert_eq!(model.errors.len(), 1);
    assert!(model.errors[0].contains("non-existent user 999"));
    
    // Try to create post for non-existent user
    let result = event_map.dispatch(AppEvent::PostCreated(PostCreated { 
        id: 100, 
        user_id: 999, 
        title: "Orphaned Post".to_string() 
    }), &mut model);
    
    assert!(result.commands.is_empty());
    assert_eq!(model.errors.len(), 2);
    assert!(model.errors[1].contains("non-existent user 999"));
}

#[test]
fn test_partial_handler_registration() {
    // Only register some handlers
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_created)
        .on(handle_post_created)
        // Skip user update/delete and post update handlers
        .build();
    
    let mut model = AppModel::default();
    
    // Registered handler should work
    let result = event_map.dispatch(AppEvent::UserCreated(UserCreated { 
        id: 1, 
        name: "Alice".to_string() 
    }), &mut model);
    
    assert_eq!(model.users.len(), 1);
    assert_eq!(result.commands.len(), 2);
    
    // Unregistered handler should return none
    let result = event_map.dispatch(AppEvent::UserUpdated(UserUpdated { 
        id: 1, 
        name: "Alice Updated".to_string(),
        email: None
    }), &mut model);
    
    assert!(result.commands.is_empty());
    assert!(result.events.is_empty());
    // Model should be unchanged except for the fact that dispatch was called
    assert_eq!(model.users.get(&1).unwrap(), "Alice"); // Still original name
}

#[test]
fn test_builder_pattern_fluency() {
    // Test that builder pattern allows flexible ordering
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_post_updated)  // Register in different order
        .on(handle_user_created)
        .on(handle_post_created)
        .on(handle_user_deleted)
        .on(handle_user_updated)
        .build();
    
    let mut model = AppModel::default();
    
    // All handlers should work regardless of registration order
    event_map.dispatch(AppEvent::UserCreated(UserCreated { 
        id: 1, 
        name: "Alice".to_string() 
    }), &mut model);
    
    event_map.dispatch(AppEvent::PostCreated(PostCreated { 
        id: 100, 
        user_id: 1, 
        title: "Test".to_string() 
    }), &mut model);
    
    assert_eq!(model.users.len(), 1);
    assert_eq!(model.posts.len(), 1);
}

#[test]
fn test_dispatch_return_value_structure() {
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_created)
        .on(handle_user_deleted)
        .build();
    
    let mut model = AppModel::default();
    
    // Create user (generates commands)
    let result = event_map.dispatch(AppEvent::UserCreated(UserCreated { 
        id: 1, 
        name: "Alice".to_string() 
    }), &mut model);
    
    assert_eq!(result.events.len(), 0);
    assert_eq!(result.commands.len(), 2);
    
    // Delete user (generates commands)
    let result = event_map.dispatch(AppEvent::UserDeleted(UserDeleted { id: 1 }), &mut model);
    
    assert_eq!(result.events.len(), 0); // No cascade events in this simple case
    assert_eq!(result.commands.len(), 2);
}

#[test]
fn test_model_state_consistency() {
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_created)
        .on(handle_user_updated)
        .on(handle_user_deleted)
        .on(handle_post_created)
        .on(handle_post_updated)
        .build();
    
    let mut model = AppModel::default();
    
    // Build up a complex state
    let events = vec![
        AppEvent::UserCreated(UserCreated { id: 1, name: "Alice".to_string() }),
        AppEvent::UserCreated(UserCreated { id: 2, name: "Bob".to_string() }),
        AppEvent::PostCreated(PostCreated { id: 100, user_id: 1, title: "Alice's Post".to_string() }),
        AppEvent::PostCreated(PostCreated { id: 101, user_id: 2, title: "Bob's Post".to_string() }),
        AppEvent::UserUpdated(UserUpdated { id: 1, name: "Alice Smith".to_string(), email: Some("alice@test.com".to_string()) }),
        AppEvent::PostUpdated(PostUpdated { id: 100, title: "Alice's Updated Post".to_string() }),
        AppEvent::UserDeleted(UserDeleted { id: 1 }), // Should cascade delete Alice's posts
    ];
    
    let mut total_commands = 0;
    let mut total_events = 0;
    
    for event in events {
        let result = event_map.dispatch(event, &mut model);
        total_commands += result.commands.len();
        total_events += result.events.len();
    }
    
    // Verify final state
    assert_eq!(model.users.len(), 1); // Only Bob remains
    assert_eq!(model.users.get(&2).unwrap(), "Bob");
    assert_eq!(model.posts.len(), 1); // Only Bob's post remains
    assert_eq!(model.event_count, 7); // All events were processed
    assert!(total_commands > 0); // Commands were generated
    assert_eq!(model.errors.len(), 0); // No errors
    
    // Verify Bob's post is intact
    let bob_post = model.posts.get(&101).unwrap();
    assert!(bob_post.contains("Bob's Post"));
    assert!(bob_post.contains("by_user_2"));
}

#[test]
fn test_event_map_immutability() {
    let event_map = EventMapBuilder::<AppEvent, AppCommand, AppModel>::new()
        .on(handle_user_created)
        .build();
    
    let mut model1 = AppModel::default();
    let mut model2 = AppModel::default();
    
    // Same event map should produce identical results for identical inputs
    let event = AppEvent::UserCreated(UserCreated { id: 1, name: "Alice".to_string() });
    
    let result1 = event_map.dispatch(event.clone(), &mut model1);
    let result2 = event_map.dispatch(event, &mut model2);
    
    assert_eq!(model1, model2);
    assert_eq!(result1.commands.len(), result2.commands.len());
    assert_eq!(result1.events.len(), result2.events.len());
}