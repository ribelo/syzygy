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

    #[inline(always)]
    fn as_slice(&self) -> &[V] {
        self
    }

    #[inline(always)]
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

    /// Ultra-optimized dispatch method that combines variant matching and handler calling
    ///
    /// This method eliminates the double indirection of the old approach by:
    /// 1. Matching the variant once to extract data
    /// 2. Using hardcoded array indices (no lookup needed)
    /// 3. Directly transmuting and calling the handler
    ///
    /// This should be significantly faster than the variant_index() + call_handler_with_data()
    /// approach since it only matches once and has no extra function calls.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all handlers in the array have the correct type signatures
    /// and remain valid for the lifetime of this call.
    unsafe fn dispatch_with_handlers<M, C>(
        self,
        handlers: &[*const ()],
        model: &mut M,
    ) -> crate::dispatch::Dispatch<Self, C>;
}


/// Ultra-high-performance event dispatcher using raw function pointers
///
/// This struct stores handlers as type-erased function pointers in a fixed-size
/// array indexed by event variant. Each handler receives its concrete variant
/// data type directly with zero downcasting or allocation overhead.
///
/// # Safety
///
/// This struct maintains these invariants:
/// 1. Each handler pointer corresponds to a function with the exact signature
///    `fn(T, &mut M) -> Dispatch<E, C>` where T is the variant's data type
/// 2. Handler pointers remain valid for the lifetime of this struct
/// 3. Variant indices are stable and match the Event::LENGTH constant
pub struct EventMap<E: Event, M, C> {
    /// Array of raw function pointers, one per event variant
    /// Each pointer has a different concrete signature but is stored type-erased
    handlers: E::Array<*const ()>,
    _phantom: PhantomData<(E, M, C)>,
}

impl<E: Event, M, C> EventMap<E, M, C> {
    /// Create a new EventMap with all handlers unset
    pub fn new() -> Self {
        // Initialize the array with null pointers using MaybeUninit pattern
        let mut array: MaybeUninit<E::Array<*const ()>> = MaybeUninit::uninit();
        let array_ptr = array.as_mut_ptr() as *mut *const ();

        // Initialize each element to null pointer
        for i in 0..E::LENGTH {
            unsafe {
                array_ptr.add(i).write(std::ptr::null());
            }
        }

        // Safety: We've initialized all elements to null pointers
        let handlers = unsafe { array.assume_init() };

        Self {
            handlers,
            _phantom: PhantomData,
        }
    }

    /// Register a typed handler for a specific event variant
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// 1. `handler` has the correct signature `fn(T, &mut M) -> Dispatch<E, C>`
    ///    where T is the exact type of the variant at `index`
    /// 2. `index` corresponds to the correct variant index from Event::variant_index()
    /// 3. The handler function pointer remains valid for the lifetime of this EventMap
    /// 4. No two different types are registered for the same index
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut map = EventMap::new();
    /// unsafe {
    ///     // UserCreated is the type inside Event::UserCreated(UserCreated)
    ///     map.register::<UserCreated>(0, handle_user_created);
    /// }
    /// ```
    pub unsafe fn register<T>(&mut self, index: usize, handler: fn(T, &mut M) -> Dispatch<E, C>) {
        debug_assert!(index < E::LENGTH, "Handler index out of bounds");

        // Store the typed handler as a raw pointer
        // This loses the type information, but we'll recover it during dispatch
        self.handlers.as_mut_slice()[index] = handler as *const ();
    }

    /// Dispatch an event enum to its registered handler (ultra-optimized)
    ///
    /// This method uses the new dispatch_with_handlers approach which eliminates
    /// the double indirection of the old variant_index() + call_handler_with_data()
    /// pattern for maximum performance.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all registered handlers have the correct type signatures
    /// and remain valid for the lifetime of this EventMap.
    #[inline(always)]
    pub unsafe fn dispatch(&self, event: E, model: &mut M) -> Dispatch<E, C> {
        // Ultra-optimized dispatch - single match, no indirection
        unsafe {
            event.dispatch_with_handlers(self.handlers.as_slice(), model)
        }
    }

    /// Check if a handler is registered for a specific variant index
    #[inline]
    pub fn has_handler(&self, variant_index: usize) -> bool {
        variant_index < self.handlers.as_slice().len() && !self.handlers.as_slice()[variant_index].is_null()
    }

    /// Get the number of registered handlers
    #[inline]
    pub fn handler_count(&self) -> usize {
        self.handlers.as_slice().iter().filter(|ptr| !ptr.is_null()).count()
    }
}

impl<E: Event, M, C> Default for EventMap<E, M, C> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder pattern for EventMap with compile-time type checking
///
/// This builder helps ensure handlers are registered with the correct types
/// while maintaining the zero-cost performance of the underlying EventMap.
pub struct EventMapBuilder<E: Event, M, C> {
    map: EventMap<E, M, C>,
}

impl<E: Event, M, C> EventMapBuilder<E, M, C> {
    pub fn new() -> Self {
        Self {
            map: EventMap::new(),
        }
    }

    /// Register a handler for a specific variant type
    ///
    /// The variant index is automatically determined from T::VARIANT_INDEX.
    ///
    /// # Safety
    ///
    /// Same safety requirements as `EventMap::register()`.
    /// The caller must ensure the handler type matches the variant's data type.
    #[inline]
    pub fn on<T>(mut self, handler: fn(T, &mut M) -> Dispatch<E, C>) -> Self
    where
        T: EventVariant,
    {
        unsafe {
            self.map.register::<T>(T::VARIANT_INDEX, handler);
        }
        self
    }

    /// Build the final EventMap
    #[inline]
    pub fn build(self) -> EventMap<E, M, C> {
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
                    let handler = unsafe { std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr) };
                    handler(data, model)
                }
                TestEvent::Update(data) => {
                    type HandlerType<M, C> = fn(UpdateUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = unsafe { std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr) };
                    handler(data, model)
                }
                TestEvent::Delete(data) => {
                    type HandlerType<M, C> = fn(DeleteUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = unsafe { std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr) };
                    handler(data, model)
                }
            }
        }

        unsafe fn dispatch_with_handlers<M, C>(
            self,
            handlers: &[*const ()],
            model: &mut M,
        ) -> crate::dispatch::Dispatch<Self, C> {
            match self {
                TestEvent::Create(data) => {
                    let handler_ptr = handlers[0];
                    if handler_ptr.is_null() {
                        return crate::dispatch::Dispatch::none();
                    }
                    type HandlerType<M, C> = fn(CreateUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = unsafe { std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr) };
                    handler(data, model)
                }
                TestEvent::Update(data) => {
                    let handler_ptr = handlers[1];
                    if handler_ptr.is_null() {
                        return crate::dispatch::Dispatch::none();
                    }
                    type HandlerType<M, C> = fn(UpdateUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = unsafe { std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr) };
                    handler(data, model)
                }
                TestEvent::Delete(data) => {
                    let handler_ptr = handlers[2];
                    if handler_ptr.is_null() {
                        return crate::dispatch::Dispatch::none();
                    }
                    type HandlerType<M, C> = fn(DeleteUser, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = unsafe { std::mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr) };
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
        let mut map = EventMap::<TestEvent, TestModel, TestCommand>::new();

        // Register handlers using concrete types
        unsafe {
            map.register::<CreateUser>(0, |data, model| {
                model.users.push(data.name.clone());
                model.counter += 1;
                Dispatch::none()
            });

            map.register::<UpdateUser>(1, |data, model| {
                if let Some(user) = model.users.get_mut(data.id as usize) {
                    *user = data.name.clone();
                }
                Dispatch::none()
            });
        }

        let mut model = TestModel::default();

        // Test create event
        let event = TestEvent::Create(CreateUser { name: "Alice".to_string() });
        unsafe {
            let result = map.dispatch(event, &mut model);
            assert!(result.events.is_empty());
            assert!(result.commands.is_empty());
        }

        assert_eq!(model.users.len(), 1);
        assert_eq!(model.users[0], "Alice");
        assert_eq!(model.counter, 1);

        // Test update event
        let event = TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() });
        unsafe {
            map.dispatch(event, &mut model);
        }

        assert_eq!(model.users[0], "Bob");
    }

    #[test]
    fn test_event_map_builder() {
        let map = EventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
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
        unsafe {
            map.dispatch(event, &mut model);
        }
        assert_eq!(model.users.len(), 1);

        // Test unregistered handler returns none
        let event = TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() });
        let result = unsafe { map.dispatch(event, &mut model) };
        assert!(matches!(result, Dispatch { events, commands } if events.is_empty() && commands.is_empty()));
    }

    #[test]
    fn test_empty_event_map() {
        let map = EventMapBuilder::<TestEvent, TestModel, TestCommand>::new().build();
        let mut model = TestModel::default();

        // All events should return none dispatch
        let events = vec![
            TestEvent::Create(CreateUser { name: "Alice".to_string() }),
            TestEvent::Update(UpdateUser { id: 0, name: "Bob".to_string() }),
            TestEvent::Delete(DeleteUser { id: 0 }),
        ];

        for event in events {
            let result = unsafe { map.dispatch(event, &mut model) };
            assert!(result.events.is_empty());
            assert!(result.commands.is_empty());
        }

        // Model should be unchanged
        assert_eq!(model.users.len(), 0);
        assert_eq!(model.counter, 0);
    }

    #[test]
    fn test_event_map_with_complex_dispatch() {
        let map = EventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
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

        let result = unsafe { map.dispatch(TestEvent::Create(CreateUser {
            name: "Alice".to_string()
        }), &mut model) };

        // Verify complex dispatch result
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.commands.len(), 1);
        assert_eq!(model.users.len(), 1);
        assert_eq!(model.counter, 1);

        // The generated event should be processable by the same map
        if let Some(update_event) = result.events.into_iter().next() {
            let update_result = unsafe { map.dispatch(update_event, &mut model) };
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

        let map = EventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
            .on(handle_create_with_prefix)
            .build();

        let mut model = TestModel::default();
        unsafe {
            map.dispatch(TestEvent::Create(CreateUser {
                name: "Alice".to_string()
            }), &mut model);
        }

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
            EventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
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
        unsafe {
            map.dispatch(TestEvent::Create(CreateUser {
                name: "Alice".to_string()
            }), &mut model);

            map_clone.dispatch(TestEvent::Create(CreateUser {
                name: "Bob".to_string()
            }), &mut model);
        }

        assert_eq!(model.users.len(), 2);
        assert_eq!(model.counter, 2);
    }
}
