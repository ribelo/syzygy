//! Test the new magic event handler API
//!
//! This test demonstrates the new `on_event<T>()` builder API that enables
//! modular, type-safe event handlers with automatic field extraction.

use syzygy::prelude::*;

// ============================================================================
// Test Model and Events
// ============================================================================

#[derive(Debug, Clone, Default)]
struct TestModel {
    users: Vec<String>,
    counter: i32,
    enabled: bool,
}

// Implement FromContainer for automatic field extraction
impl FromContainer<TestModel> for Vec<String> {
    fn from_container(model: &TestModel) -> Self {
        model.users.clone()
    }
}

impl FromContainer<TestModel> for i32 {
    fn from_container(model: &TestModel) -> Self {
        model.counter
    }
}

impl FromContainer<TestModel> for bool {
    fn from_container(model: &TestModel) -> Self {
        model.enabled
    }
}

// Event types
#[derive(Debug, Clone, PartialEq)]
struct CreateUser {
    name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct UpdateCounter {
    increment: i32,
}

#[derive(Debug, Clone, PartialEq)]
enum TestEvent {
    CreateUser(CreateUser),
    UpdateCounter(UpdateCounter),
    UserCreated { name: String },
    CounterUpdated { value: i32 },
}

#[derive(Debug, Clone, PartialEq)]
enum TestCommand {
    SaveUser { name: String },
    LogMessage { message: String },
}

// Implement From traits for event conversion
impl From<CreateUser> for TestEvent {
    fn from(event: CreateUser) -> Self {
        TestEvent::CreateUser(event)
    }
}

impl From<UpdateCounter> for TestEvent {
    fn from(event: UpdateCounter) -> Self {
        TestEvent::UpdateCounter(event)
    }
}

impl TryFrom<TestEvent> for CreateUser {
    type Error = ();
    
    fn try_from(event: TestEvent) -> Result<Self, Self::Error> {
        match event {
            TestEvent::CreateUser(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

impl TryFrom<TestEvent> for UpdateCounter {
    type Error = ();
    
    fn try_from(event: TestEvent) -> Result<Self, Self::Error> {
        match event {
            TestEvent::UpdateCounter(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

// ============================================================================
// Magic Event Handlers
// ============================================================================

// Handler that extracts users list and counter from model
fn handle_create_user(
    event: CreateUser,
    users: Vec<String>,
    counter: i32,
) -> Dispatch<TestEvent, TestCommand> {
    let new_name = format!("{}_{}", event.name, counter);
    
    Dispatch::new(
        vec![TestEvent::UserCreated { name: new_name.clone() }],
        vec![TestCommand::SaveUser { name: new_name }],
    )
}

// Handler that only needs the counter
fn handle_update_counter(
    event: UpdateCounter,
    counter: i32,
) -> Dispatch<TestEvent, TestCommand> {
    let new_value = counter + event.increment;
    
    Dispatch::new(
        vec![TestEvent::CounterUpdated { value: new_value }],
        vec![TestCommand::LogMessage { 
            message: format!("Counter updated from {} to {}", counter, new_value) 
        }],
    )
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_magic_event_handler_api() {
    // Test that the builder accepts the new on_event API
    let _builder = Syzygy::builder()
        .model(TestModel::default())
        .on_event::<CreateUser>(|_event| Dispatch::<TestEvent, TestCommand>::none())
        .on_event::<UpdateCounter>(|_event| Dispatch::<TestEvent, TestCommand>::none());
    
    // For now, we're just testing that the API compiles and accepts the handlers
    // Once crabtime integration is complete, this will test the actual functionality
}

#[test]
fn test_event_handler_metadata_collection() {
    let builder = Syzygy::builder()
        .model(TestModel::default())
        .on_event::<CreateUser>(|_event| Dispatch::<TestEvent, TestCommand>::none())
        .on_event::<UpdateCounter>(|_event| Dispatch::<TestEvent, TestCommand>::none());
    
    // Test that metadata is collected correctly
    assert_eq!(builder.event_handler_storage.handlers.len(), 2);
    
    let handler_names: Vec<&String> = builder.event_handler_storage.handlers
        .iter()
        .map(|h| &h.event_type_name)
        .collect();
    
    assert!(handler_names.contains(&&"CreateUser".to_string()));
    assert!(handler_names.contains(&&"UpdateCounter".to_string()));
}

#[test]
fn test_handler_function_name_generation() {
    let builder = Syzygy::builder()
        .model(TestModel::default())
        .on_event::<CreateUser>(|_event| Dispatch::<TestEvent, TestCommand>::none());
    
    let handler = &builder.event_handler_storage.handlers[0];
    assert_eq!(handler.handler_function_name, "handle_createuser");
    assert_eq!(handler.event_type_name, "CreateUser");
}

#[test]
fn test_magic_handler_direct_call() {
    let mut model = TestModel {
        users: vec!["alice".to_string()],
        counter: 42,
        enabled: true,
    };
    
    let event = CreateUser { name: "bob".to_string() };
    
    // Test direct magic handler call
    let result = handle_create_user.call_with_event(event, &mut model);
    
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.commands.len(), 1);
    
    match &result.events[0] {
        TestEvent::UserCreated { name } => {
            assert_eq!(name, "bob_42");
        }
        _ => panic!("Expected UserCreated event"),
    }
    
    match &result.commands[0] {
        TestCommand::SaveUser { name } => {
            assert_eq!(name, "bob_42");
        }
        _ => panic!("Expected SaveUser command"),
    }
}

#[test]
fn test_multiple_handlers_different_parameters() {
    let mut model = TestModel {
        users: vec![],
        counter: 10,
        enabled: true,
    };
    
    // Test handler that needs users and counter
    let create_event = CreateUser { name: "test".to_string() };
    let create_result = handle_create_user.call_with_event(create_event, &mut model);
    
    assert_eq!(create_result.events.len(), 1);
    
    // Test handler that only needs counter
    let update_event = UpdateCounter { increment: 5 };
    let update_result = handle_update_counter.call_with_event(update_event, &mut model);
    
    assert_eq!(update_result.events.len(), 1);
    
    match &update_result.events[0] {
        TestEvent::CounterUpdated { value } => {
            assert_eq!(*value, 15); // 10 + 5
        }
        _ => panic!("Expected CounterUpdated event"),
    }
}