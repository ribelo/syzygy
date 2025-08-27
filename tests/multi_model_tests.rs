//! Tests for multi-model functionality
//!
//! These tests are only compiled when the "multi-model" feature is enabled.

#![cfg(feature = "multi-model")]

use syzygy::prelude::*;

// Test models
#[derive(Debug, Clone, PartialEq)]
struct UserModel {
    id: u32,
    name: String,
    email: String,
}

impl Model for UserModel {
    const IDX: usize = 0;
}

#[derive(Debug, Clone, PartialEq)]
struct SessionModel {
    token: String,
    user_id: u32,
    expires_at: u64,
}

impl Model for SessionModel {
    const IDX: usize = 1;
}

#[derive(Debug, Clone, PartialEq)]
struct ConfigModel {
    theme: String,
    language: String,
    notifications: bool,
}

impl Model for ConfigModel {
    const IDX: usize = 2;
}

// Test events
#[derive(Debug, Clone)]
enum TestEvent {
    UpdateUser { name: String },
    UpdateSession { token: String },
    UpdateConfig { theme: String },
}

// Test effects
#[derive(Debug, Clone)]
enum TestEffect {
    SaveUser,
    SaveSession,
    SaveConfig,
}

// Test app
#[derive(Default)]
struct MultiModelApp;

impl App for MultiModelApp {
    type Event = TestEvent;
    type Model = (); // Not used in multi-model mode
    type Effect = TestEffect;
    type Resources = ();

    fn update(
        &self,
        event: Self::Event,
        _model: &mut Self::Model,
    ) -> Command<Self::Event, Self::Effect> {
        // In multi-model mode, the actual model access would happen differently
        // For now, just return appropriate effects based on the event
        match event {
            TestEvent::UpdateUser { .. } => Command::effect(TestEffect::SaveUser),
            TestEvent::UpdateSession { .. } => Command::effect(TestEffect::SaveSession),
            TestEvent::UpdateConfig { .. } => Command::effect(TestEffect::SaveConfig),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_registry_basic_operations() {
        let mut registry = ModelRegistry::new();

        // Insert models
        registry.insert(UserModel {
            id: 123,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        });

        registry.insert(SessionModel {
            token: "abc123".to_string(),
            user_id: 123,
            expires_at: 1234567890,
        });

        // Retrieve models
        let user = registry.get::<UserModel>();
        assert_eq!(user.id, 123);
        assert_eq!(user.name, "Alice");

        let session = registry.get::<SessionModel>();
        assert_eq!(session.token, "abc123");
        assert_eq!(session.user_id, 123);
    }

    #[test]
    fn test_builder_multi_model_chaining() {
        let user = UserModel {
            id: 123,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        };

        let session = SessionModel {
            token: "abc123".to_string(),
            user_id: 123,
            expires_at: 1234567890,
        };

        let config = ConfigModel {
            theme: "dark".to_string(),
            language: "en".to_string(),
            notifications: true,
        };

        // Build system with multiple models
        let (core, _shell) = Syzygy::builder()
            .app(MultiModelApp::default())
            .model(user)
            .model(session)
            .model(config)
            .build();

        // Verify we can access the core (compilation test)
        let _ = core.event_sender();
    }

    #[test]
    fn test_model_registry_mutability() {
        let mut registry = ModelRegistry::new();

        registry.insert(UserModel {
            id: 123,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        });

        // Test mutable access
        {
            let user_mut = registry.get_mut::<UserModel>();
            user_mut.name = "Bob".to_string();
            user_mut.email = "bob@example.com".to_string();
        }

        // Verify changes
        let user = registry.get::<UserModel>();
        assert_eq!(user.name, "Bob");
        assert_eq!(user.email, "bob@example.com");
        assert_eq!(user.id, 123); // Unchanged
    }

    #[test]
    fn test_cached_getters() {
        let mut registry = ModelRegistry::new();

        registry.insert(UserModel {
            id: 123,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        });

        registry.insert(SessionModel {
            token: "abc123".to_string(),
            user_id: 123,
            expires_at: 1234567890,
        });

        // Create cached getters
        let user_getter = registry.cache::<UserModel>();
        let session_getter = registry.cache::<SessionModel>();

        // Use cached getters
        let user = user_getter.get(&registry);
        let session = session_getter.get(&registry);

        assert_eq!(user.name, "Alice");
        assert_eq!(session.token, "abc123");
    }

    #[test]
    fn test_model_registry_len_and_empty() {
        let mut registry = ModelRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        registry.insert(UserModel {
            id: 123,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        });

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.insert(SessionModel {
            token: "abc123".to_string(),
            user_id: 123,
            expires_at: 1234567890,
        });

        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_try_get_missing_model() {
        let registry = ModelRegistry::new();
        assert!(registry.try_get::<UserModel>().is_none());
    }

    #[test]
    #[should_panic(expected = "model type not present in ModelRegistry")]
    fn test_get_missing_model_panics() {
        let registry = ModelRegistry::new();
        let _ = registry.get::<UserModel>();
    }

    #[test]
    unsafe fn test_unchecked_access() {
        let mut registry = ModelRegistry::new();

        registry.insert(UserModel {
            id: 123,
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
        });

        // SAFETY: We just inserted the model
        let user = registry.get_unchecked::<UserModel>();
        assert_eq!(user.id, 123);

        // Test cached unchecked access too
        let getter = registry.cache::<UserModel>();
        // SAFETY: We know the model exists
        let user2 = getter.get_unchecked(&registry);
        assert_eq!(user2.name, "Alice");
    }

    #[test]
    #[should_panic(
        expected = "At least one model must be added before building in multi-model mode"
    )]
    fn test_builder_requires_models() {
        let _system = Syzygy::builder().app(MultiModelApp::default()).build(); // Should panic - no models added
    }
}

// ============================================================================
// Tests for Reference Extraction Bug
// ============================================================================

use syzygy::magic_handler::EventMagicHandler;

#[derive(Debug, Clone, Default)]
struct FirstModel {
    value: i32,
}

#[derive(Debug, Clone, Default)]
struct SecondModel {
    name: String,
}

#[derive(Debug, Clone)]
enum RefTestEvent {
    Process,
}

#[derive(Debug, Clone)]
enum RefTestEffect {
    Log { message: String },
}

#[test]
fn test_reference_extraction_bug_second_model() {
    // BUG TEST: Create storage with FirstModel at Here and SecondModel at There<Here>
    let mut storage = EmptyStorage
        .with_model(FirstModel { value: 42 })
        .with_model(SecondModel {
            name: "test".to_string(),
        });

    let mut ctx = EventContext::<RefTestEvent, RefTestEffect, _>::new(&mut storage);

    // Handler that tries to extract SecondModel by reference
    // This will get wrong data because &SecondModel assumes Here position
    // but SecondModel is actually at There<Here> position
    fn handler_with_second_model(
        event: RefTestEvent,
        second: &SecondModel, // BUG: assumes SecondModel is at Here
    ) -> Command<RefTestEvent, RefTestEffect> {
        match event {
            RefTestEvent::Process => Command::effect(RefTestEffect::Log {
                message: format!("Second model name: {}", second.name),
            }),
        }
    }

    let event = RefTestEvent::Process;

    // This compiles but will likely access wrong data or panic
    let _command = handler_with_second_model.call_with_event(event, &mut ctx);
}

#[test]
fn test_reference_extraction_first_model_works() {
    // Create storage with FirstModel at Here and SecondModel at There<Here>
    let mut storage = EmptyStorage
        .with_model(FirstModel { value: 99 })
        .with_model(SecondModel {
            name: "working".to_string(),
        });

    let mut ctx = EventContext::<RefTestEvent, RefTestEffect, _>::new(&mut storage);

    // Handler that extracts FirstModel by reference - this should work
    fn handler_with_first_model(
        event: RefTestEvent,
        first: &FirstModel,
    ) -> Command<RefTestEvent, RefTestEffect> {
        match event {
            RefTestEvent::Process => Command::effect(RefTestEffect::Log {
                message: format!("First model value: {}", first.value),
            }),
        }
    }

    let event = RefTestEvent::Process;
    let _command = handler_with_first_model.call_with_event(event, &mut ctx);
}

#[test]
fn test_correct_manual_model_access() {
    use syzygy::storage::storage::{Here, There};

    // Show the CORRECT way to access models at different positions
    let mut storage = EmptyStorage
        .with_model(FirstModel { value: 123 })
        .with_model(SecondModel {
            name: "manual".to_string(),
        });

    let ctx = EventContext::<RefTestEvent, RefTestEffect, _>::new(&mut storage);

    // Correct manual access with proper position types
    let first: &FirstModel = ctx.model::<FirstModel, Here>();
    let second: &SecondModel = ctx.model::<SecondModel, There<Here>>();

    assert_eq!(first.value, 123);
    assert_eq!(second.name, "manual");
}
