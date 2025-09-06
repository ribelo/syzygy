//! Dispatch trait for high-performance event routing

use crate::command::Command;
use crate::event_context::EventContext;
use crate::event_handler_map::EventHandlerMap;

/// Trait for dispatching events using EventHandlerMap
pub trait Dispatch {
    /// Get the variant index for this event
    fn variant_index(&self) -> usize;
    
    /// Dispatch this event using the provided handler map
    fn dispatch_with_handlers<Effect, Storage>(
        self,
        handlers: &EventHandlerMap<Self, Effect, Storage>,
        context: &EventContext<Self, Effect, Storage>,
    ) -> Command<Self, Effect>
    where
        Self: Sized;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use syzygy_macros::Dispatch;

    #[derive(Debug, Clone, PartialEq)]
    struct UserCreated {
        id: u32,
        name: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct UserDeleted {
        id: u32,
    }

    #[derive(Dispatch, Debug, Clone, PartialEq)]
    enum TestEvent {
        Created(UserCreated),
        Deleted(UserDeleted),
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log(String),
    }

    #[derive(Default)]
    struct TestModel {
        count: u32,
    }

    fn handle_user_created(_data: UserCreated, _model: &mut TestModel) -> Command<TestEvent, TestEffect> {
        Command::none()
    }

    fn handle_user_deleted(_data: UserDeleted, _model: &mut TestModel) -> Command<TestEvent, TestEffect> {
        Command::none()
    }

    #[test]
    fn test_dispatch_trait() {
        let event1 = TestEvent::Created(UserCreated { id: 1, name: "Alice".to_string() });
        let event2 = TestEvent::Deleted(UserDeleted { id: 1 });
        
        // Test variant_index method
        assert_eq!(event1.variant_index(), UserCreated::INDEX);
        assert_eq!(event2.variant_index(), UserDeleted::INDEX);
        
        // Test that indices are different
        assert_ne!(event1.variant_index(), event2.variant_index());
    }

    #[test]
    fn test_dispatch_with_handlers() {
        let mut storage = EmptyStorage.with_model(TestModel::default());
        let context = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let handler_map = crate::event_handler_map::HandlerMapBuilder::new()
            .on(handle_user_created)
            .on(handle_user_deleted)
            .build();

        let event = TestEvent::Created(UserCreated { id: 1, name: "Alice".to_string() });
        let result = event.dispatch_with_handlers(&handler_map, &context);
        
        // Should return a valid command
        assert_eq!(result.count_events(), 0);
        assert_eq!(result.count_effects(), 0);
    }
}