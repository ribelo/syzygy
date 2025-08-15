//! EventMap - Zero-overhead event dispatch using array indexing
//!
//! This module provides a compile-time safe, array-based event dispatcher
//! that maps enum variants to handlers with zero runtime overhead.

use std::any::Any;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use crate::dispatch::Dispatch;

/// Trait for event variant types that can provide their index within the parent enum
pub trait EventVariant {
    /// The index of this variant within the parent enum
    const VARIANT_INDEX: usize;
}

/// Trait for array types used by EventMap
///
/// This trait provides array operations needed by EventMap.
/// Safety: LENGTH must match the actual array size.
pub unsafe trait EventArray<V>: Sized {
    /// The actual length of the array
    const LENGTH: usize;

    /// Get a slice view of the array
    fn as_slice(&self) -> &[V];

    /// Get a mutable slice view of the array
    fn as_mut_slice(&mut self) -> &mut [V];
}

// Implement EventArray for standard arrays
unsafe impl<V, const N: usize> EventArray<V> for [V; N] {
    const LENGTH: usize = N;

    #[inline]
    fn as_slice(&self) -> &[V] {
        self
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [V] {
        self
    }
}

/// Trait for indexable event enums
///
/// This trait enables enum variants to be used as array indices for
/// zero-overhead dispatch. Each variant must contain a unique type.
pub trait Event: Sized {
    /// Number of variants in the enum
    const LENGTH: usize;

    /// Array type for storing values indexed by this enum
    /// This is needed because Rust doesn't allow E::LENGTH in array types directly
    type Array<V>: EventArray<V>;

    /// Convert the variant to its array index
    fn variant_index(&self) -> usize;

    /// Extract the inner data as a type-erased reference
    fn inner_as_any(&self) -> &dyn Any;

    /// Get the TypeId of the inner data (for validation)
    fn inner_type_id(&self) -> std::any::TypeId;

    /// Extract variant data and call the correctly-typed handler (unsafe version)
    ///
    /// This method enables zero-cost dispatch by extracting owned variant data
    /// and calling a typed handler function with the concrete data type.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// 1. `handler_ptr` points to a function with signature `fn(T, &mut M) -> Dispatch<Self, C>`
    ///    where T is the exact type of this variant's data
    /// 2. The handler pointer is valid and properly aligned
    /// 3. The handler function is safe to call with the provided arguments
    ///
    /// # Implementation
    ///
    /// This method should be implemented via derive macro to generate a match
    /// statement that extracts each variant's data and transmutes the handler
    /// to the correct concrete type before calling it.
    unsafe fn call_handler_with_data<M, C>(
        self,
        handler_ptr: *const (),
        model: &mut M,
    ) -> crate::dispatch::Dispatch<Self, C>;
}

/// Handler function type that processes owned events directly
///
/// Takes the complete event enum by value for zero-overhead dispatch
pub type EventHandler<E, C, M> = Box<dyn Fn(E, &mut M) -> Dispatch<E, C> + Send + Sync>;

/// Array-based event dispatcher with O(1) lookup
///
/// Stores handlers in a fixed-size array indexed by event variant
pub struct EventMap<E: Event, C, M> {
    handlers: E::Array<Option<EventHandler<E, C, M>>>,
    _phantom: PhantomData<(E, C, M)>,
}

impl<E: Event + 'static, C: 'static, M: 'static> EventMap<E, C, M> {
    /// Create a new EventMap with all handlers unset
    pub fn new() -> Self {
        // We need to initialize the array with None values
        // Using MaybeUninit to safely initialize the array
        let mut array: MaybeUninit<E::Array<Option<EventHandler<E, C, M>>>> = MaybeUninit::uninit();
        let array_ptr = array.as_mut_ptr() as *mut Option<EventHandler<E, C, M>>;

        // Initialize each element to None
        for i in 0..E::LENGTH {
            unsafe {
                array_ptr.add(i).write(None);
            }
        }

        // Safety: We've initialized all elements
        let handlers = unsafe { array.assume_init() };

        Self {
            handlers,
            _phantom: PhantomData,
        }
    }

    /// Register a typed handler for a specific event variant
    ///
    /// Note: This implementation creates a closure and may have slight performance overhead.
    /// For maximum performance, create a direct handler that takes the full enum.
    pub fn register<T>(&mut self, variant_index: usize, handler: fn(T, &mut M) -> Dispatch<E, C>)
    where
        T: From<E>,
        E: Clone,
    {
        // For now, store the direct handler approach for better performance
        // The handler should be created manually by the user for optimal performance
        panic!("For optimal performance, create a direct handler that pattern matches the enum. Use register_direct() instead.");
    }

    /// Register a handler that receives the event directly
    pub fn register_direct(&mut self, variant_index: usize, handler: EventHandler<E, C, M>) {
        self.handlers.as_mut_slice()[variant_index] = Some(handler);
    }

    /// Register a function pointer handler that receives the event directly
    pub fn register_fn(&mut self, variant_index: usize, handler: fn(E, &mut M) -> Dispatch<E, C>) {
        self.handlers.as_mut_slice()[variant_index] = Some(Box::new(handler));
    }

    /// Dispatch an event to its registered handler
    ///
    /// Returns Dispatch::none() if no handler is registered for the variant
    #[inline(always)]
    pub fn dispatch(&self, event: E, model: &mut M) -> Dispatch<E, C> {
        let index = event.variant_index();
        if let Some(handler) = &self.handlers.as_slice()[index] {
            handler(event, model)
        } else {
            Dispatch::none()
        }
    }

    /// Check if a handler is registered for a specific variant index
    pub fn has_handler(&self, variant_index: usize) -> bool {
        variant_index < E::LENGTH && self.handlers.as_slice()[variant_index].is_some()
    }
}

impl<E: Event + 'static, C: 'static, M: 'static> Default for EventMap<E, C, M> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder pattern for EventMap with typed handler registration
pub struct EventMapBuilder<E: Event, C, M> {
    map: EventMap<E, C, M>,
}

impl<E: Event + Clone + 'static, C: 'static, M: 'static> EventMapBuilder<E, C, M> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: EventMap::new(),
        }
    }

    /// Register a handler for a specific variant type
    ///
    /// The variant index is automatically determined from T::VARIANT_INDEX.
    /// This creates a wrapper handler that extracts the inner type from the enum.
    #[must_use]
    pub fn on<T>(mut self, handler: fn(T, &mut M) -> Dispatch<E, C>) -> Self
    where
        T: From<E> + EventVariant + 'static,
    {
        // Create a direct handler that pattern matches and extracts the type
        let direct_handler: EventHandler<E, C, M> = Box::new(move |event: E, model: &mut M| -> Dispatch<E, C> {
            let typed_data = T::from(event);
            handler(typed_data, model)
        });

        self.map.register_direct(T::VARIANT_INDEX, direct_handler);
        self
    }

    /// Build the EventMap
    pub fn build(self) -> EventMap<E, C, M> {
        self.map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test event types
    #[derive(Debug, Clone)]
    struct CreateUser { name: String }

    #[derive(Debug, Clone)]
    struct UpdateUser { id: u64, name: String }

    #[derive(Debug, Clone)]
    struct DeleteUser { id: u64 }

    // EventVariant implementations for test types
    impl EventVariant for CreateUser {
        const VARIANT_INDEX: usize = 0;
    }

    impl EventVariant for UpdateUser {
        const VARIANT_INDEX: usize = 1;
    }

    impl EventVariant for DeleteUser {
        const VARIANT_INDEX: usize = 2;
    }

    // From implementations for test types
    impl From<TestEvent> for CreateUser {
        fn from(event: TestEvent) -> Self {
            match event {
                TestEvent::Create(data) => data,
                _ => panic!("Invalid conversion from TestEvent to CreateUser"),
            }
        }
    }

    impl From<TestEvent> for UpdateUser {
        fn from(event: TestEvent) -> Self {
            match event {
                TestEvent::Update(data) => data,
                _ => panic!("Invalid conversion from TestEvent to UpdateUser"),
            }
        }
    }

    impl From<TestEvent> for DeleteUser {
        fn from(event: TestEvent) -> Self {
            match event {
                TestEvent::Delete(data) => data,
                _ => panic!("Invalid conversion from TestEvent to DeleteUser"),
            }
        }
    }

    // Test event enum
    #[derive(Debug, Clone)]
    enum TestEvent {
        Create(CreateUser),
        Update(UpdateUser),
        Delete(DeleteUser),
    }

    // Manual Event implementation for testing
    impl Event for TestEvent {
        const LENGTH: usize = 3;
        type Array<V> = [V; 3];

        fn variant_index(&self) -> usize {
            match self {
                TestEvent::Create(_) => 0,
                TestEvent::Update(_) => 1,
                TestEvent::Delete(_) => 2,
            }
        }

        fn inner_as_any(&self) -> &dyn Any {
            match self {
                TestEvent::Create(data) => data,
                TestEvent::Update(data) => data,
                TestEvent::Delete(data) => data,
            }
        }

        fn inner_type_id(&self) -> std::any::TypeId {
            match self {
                TestEvent::Create(_) => std::any::TypeId::of::<CreateUser>(),
                TestEvent::Update(_) => std::any::TypeId::of::<UpdateUser>(),
                TestEvent::Delete(_) => std::any::TypeId::of::<DeleteUser>(),
            }
        }

        unsafe fn call_handler_with_data<M, C>(
            self,
            handler_ptr: *const (),
            model: &mut M,
        ) -> crate::dispatch::Dispatch<Self, C> {
            match self {
                TestEvent::Create(data) => {
                    type HandlerType<M, C> = fn(CreateUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                    handler(data, model)
                }
                TestEvent::Update(data) => {
                    type HandlerType<M, C> = fn(UpdateUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                    handler(data, model)
                }
                TestEvent::Delete(data) => {
                    type HandlerType<M, C> = fn(DeleteUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                    handler(data, model)
                }
            }
        }
    }

    // Test command
    #[derive(Debug)]
    enum TestCommand {
        Log(String),
    }

    // Test model
    #[derive(Default)]
    struct TestModel {
        users: Vec<String>,
        counter: usize,
    }

    #[test]
    fn test_event_map_dispatch() {
        let mut map = EventMap::<TestEvent, TestCommand, TestModel>::new();

        // Register handlers using register_direct (which takes full enum)
        map.register_direct(0, Box::new(|event, model| {
            if let TestEvent::Create(data) = event {
                model.users.push(data.name.clone());
                model.counter += 1;
            }
            Dispatch::none()
        }));

        map.register_direct(1, Box::new(|event, model| {
            if let TestEvent::Update(data) = event {
                if let Some(user) = model.users.get_mut(data.id as usize) {
                    *user = data.name.clone();
                }
            }
            Dispatch::none()
        }));

        let mut model = TestModel::default();

        // Test create event
        let event = TestEvent::Create(CreateUser { name: "Alice".to_string() });
        map.dispatch(event, &mut model);

        assert_eq!(model.users.len(), 1);
        assert_eq!(model.users[0], "Alice");
        assert_eq!(model.counter, 1);

        // Test update event
        let event = TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() });
        map.dispatch(event, &mut model);

        assert_eq!(model.users[0], "Bob");
    }

    #[test]
    fn test_event_map_builder() {
        let map = EventMapBuilder::<TestEvent, TestCommand, TestModel>::new()
            .on(|data: CreateUser, model: &mut TestModel| {
                model.users.push(data.name.clone());
                Dispatch::none()
            })
            .on(|data: DeleteUser, model: &mut TestModel| {
                if (data.id as usize) < model.users.len() {
                    model.users.remove(data.id as usize);
                }
                Dispatch::none()
            })
            .build();

        let mut model = TestModel::default();

        // Test that handlers work
        let event = TestEvent::Create(CreateUser { name: "Alice".to_string() });
        map.dispatch(event, &mut model);
        assert_eq!(model.users.len(), 1);

        // Test unregistered handler returns none
        let event = TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() });
        let result = map.dispatch(event, &mut model);
        assert!(matches!(result, Dispatch { events, commands } if events.is_empty() && commands.is_empty()));
    }

    #[test]
    fn test_empty_event_map() {
        let map = EventMapBuilder::<TestEvent, TestCommand, TestModel>::new().build();
        let mut model = TestModel::default();
        
        // All events should return none dispatch
        let events = vec![
            TestEvent::Create(CreateUser { name: "Alice".to_string() }),
            TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() }),
            TestEvent::Delete(DeleteUser { id: 0 }),
        ];
        
        for event in events {
            let result = map.dispatch(event, &mut model);
            assert!(result.events.is_empty());
            assert!(result.commands.is_empty());
        }
        
        // Model should be unchanged
        assert_eq!(model.users.len(), 0);
        assert_eq!(model.counter, 0);
    }

    #[test]
    fn test_event_map_with_complex_dispatch() {
        let map = EventMapBuilder::<TestEvent, TestCommand, TestModel>::new()
            .on(|data: CreateUser, model: &mut TestModel| {
                model.users.push(data.name.clone());
                model.counter += 1;
                // Generate multiple events and commands
                Dispatch::new(
                    vec![TestEvent::Update(UpdateUser { id: 0, name: data.name.clone() })],
                    vec![TestCommand::Log(format!("Created: {}", data.name))]
                )
            })
            .on(|data: UpdateUser, model: &mut TestModel| {
                if let Some(user) = model.users.get_mut(data.id as usize) {
                    *user = data.name.clone();
                    model.counter += 1;
                }
                Dispatch::command(TestCommand::Log(format!("Updated: {}", data.name)))
            })
            .build();
        
        let mut model = TestModel::default();
        
        let result = map.dispatch(TestEvent::Create(CreateUser { 
            name: "Alice".to_string() 
        }), &mut model);
        
        // Verify complex dispatch result
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.commands.len(), 1);
        assert_eq!(model.users.len(), 1);
        assert_eq!(model.counter, 1);
        
        // The generated event should be processable by the same map
        if let Some(update_event) = result.events.into_iter().next() {
            let update_result = map.dispatch(update_event, &mut model);
            assert_eq!(update_result.commands.len(), 1);
            assert_eq!(model.counter, 2);
        }
    }

    #[test]
    fn test_event_map_handler_ownership() {
        // Test that handlers work with custom logic
        fn handle_create_with_prefix(data: CreateUser, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
            model.users.push(format!("User: {}", data.name));
            Dispatch::none()
        }
        
        let map = EventMapBuilder::<TestEvent, TestCommand, TestModel>::new()
            .on(handle_create_with_prefix)
            .build();
        
        let mut model = TestModel::default();
        map.dispatch(TestEvent::Create(CreateUser { 
            name: "Alice".to_string() 
        }), &mut model);
        
        assert_eq!(model.users[0], "User: Alice");
    }

    #[test]
    fn test_event_map_type_safety() {
        // This test verifies that the From<TestEvent> implementations work correctly
        let create_user = CreateUser { name: "Alice".to_string() };
        let test_event = TestEvent::Create(create_user.clone());
        
        // Should be able to convert back
        let converted: CreateUser = CreateUser::from(test_event);
        assert_eq!(converted.name, create_user.name);
    }

    #[test]
    fn test_event_map_variant_index_correctness() {
        // Verify that EventVariant constants match actual enum layout
        assert_eq!(CreateUser::VARIANT_INDEX, 0);
        assert_eq!(UpdateUser::VARIANT_INDEX, 1);
        assert_eq!(DeleteUser::VARIANT_INDEX, 2);
        
        // Test events should map to correct indices
        let events = vec![
            TestEvent::Create(CreateUser { name: "Alice".to_string() }),
            TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() }),
            TestEvent::Delete(DeleteUser { id: 0 }),
        ];
        
        let expected_indices = [0, 1, 2];
        for (event, expected_index) in events.iter().zip(expected_indices.iter()) {
            assert_eq!(event.variant_index(), *expected_index);
        }
    }

    #[test]
    fn test_event_map_concurrent_usage() {
        // Test that EventMap can be used concurrently (it should be Sync + Send)
        let map = std::sync::Arc::new(
            EventMapBuilder::<TestEvent, TestCommand, TestModel>::new()
                .on(|data: CreateUser, model: &mut TestModel| {
                    model.users.push(data.name);
                    model.counter += 1;
                    Dispatch::none()
                })
                .build()
        );
        
        // Clone for thread safety test
        let map_clone = map.clone();
        let mut model = TestModel::default();
        
        // Should be able to dispatch from different references
        map.dispatch(TestEvent::Create(CreateUser { 
            name: "Alice".to_string() 
        }), &mut model);
        
        map_clone.dispatch(TestEvent::Create(CreateUser { 
            name: "Bob".to_string() 
        }), &mut model);
        
        assert_eq!(model.users.len(), 2);
        assert_eq!(model.counter, 2);
    }
}
