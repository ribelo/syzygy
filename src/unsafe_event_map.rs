//! UnsafeEventMap - Zero-cost event dispatch using raw function pointers
//!
//! This module provides an ultra-high-performance event dispatcher that uses
//! raw function pointers to eliminate all allocation and dynamic dispatch overhead.
//!
//! # Safety
//!
//! This module uses unsafe code to achieve maximum performance. All unsafe operations
//! are carefully documented and follow strict invariants to ensure memory safety.

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use crate::dispatch::Dispatch;
use crate::event_map::{Event, EventArray};

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
pub struct UnsafeEventMap<E: Event, M, C> {
    /// Array of raw function pointers, one per event variant
    /// Each pointer has a different concrete signature but is stored type-erased
    handlers: E::Array<*const ()>,
    _phantom: PhantomData<(E, M, C)>,
}

impl<E: Event, M, C> UnsafeEventMap<E, M, C> {
    /// Create a new UnsafeEventMap with all handlers unset
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
    /// let mut map = UnsafeEventMap::new();
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

    /// Dispatch an event to its registered handler
    ///
    /// Returns `Dispatch::none()` if no handler is registered for the event's variant.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// 1. All handlers were registered with the correct types using `register()`
    /// 2. The Event implementation's `call_handler_with_data()` method correctly
    ///    matches variants to their data types
    ///
    /// # Performance
    ///
    /// This method performs:
    /// 1. One array bounds check (variant_index)
    /// 2. One null pointer check
    /// 3. One direct function call with zero indirection
    ///
    /// The generated code should be nearly identical to a match statement.
    pub unsafe fn dispatch(&self, event: E, model: &mut M) -> Dispatch<E, C> {
        let index = event.variant_index();

        // Bounds check - this should be optimized away if LENGTH is known at compile time
        debug_assert!(index < self.handlers.as_slice().len(), "Event variant index out of bounds");

        let handler_ptr = self.handlers.as_slice()[index];

        // Check if handler is registered
        if handler_ptr.is_null() {
            return Dispatch::none();
        }

        // Delegate to the Event trait to extract variant data and call the typed handler
        // This is where the magic happens - the Event impl knows the concrete types
        unsafe {
            event.call_handler_with_data(handler_ptr, model)
        }
    }

    /// Check if a handler is registered for a specific variant index
    pub fn has_handler(&self, index: usize) -> bool {
        index < self.handlers.as_slice().len() && !self.handlers.as_slice()[index].is_null()
    }

    /// Get the number of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handlers.as_slice().iter().filter(|ptr| !ptr.is_null()).count()
    }
}

impl<E: Event, M, C> Default for UnsafeEventMap<E, M, C> {
    fn default() -> Self {
        Self::new()
    }
}


/// Builder pattern for UnsafeEventMap with compile-time type checking
///
/// This builder helps ensure handlers are registered with the correct types
/// while maintaining the zero-cost performance of the underlying UnsafeEventMap.
pub struct UnsafeEventMapBuilder<E: Event, M, C> {
    map: UnsafeEventMap<E, M, C>,
}

impl<E: Event, M, C> UnsafeEventMapBuilder<E, M, C> {
    pub fn new() -> Self {
        Self {
            map: UnsafeEventMap::new(),
        }
    }

    /// Register a handler for a specific variant type
    ///
    /// The variant index is automatically determined from T::VARIANT_INDEX.
    ///
    /// # Safety
    ///
    /// Same safety requirements as `UnsafeEventMap::register()`.
    /// The caller must ensure the handler type matches the variant's data type.
    pub fn on<T>(mut self, handler: fn(T, &mut M) -> Dispatch<E, C>) -> Self
    where
        T: crate::event_map::EventVariant,
    {
        unsafe {
            self.map.register::<T>(T::VARIANT_INDEX, handler);
        }
        self
    }

    /// Build the final UnsafeEventMap
    pub fn build(self) -> UnsafeEventMap<E, M, C> {
        self.map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_map::EventVariant;
    use std::mem;

    // Test event data types - each must be unique
    #[derive(Debug, Clone, PartialEq)]
    struct CreateData { name: String }

    #[derive(Debug, Clone, PartialEq)]
    struct UpdateData { id: u64, name: String }

    #[derive(Debug, Clone, PartialEq)]
    struct DeleteData { id: u64 }

    // EventVariant implementations for test types
    impl crate::event_map::EventVariant for CreateData {
        const VARIANT_INDEX: usize = 0;
    }

    impl crate::event_map::EventVariant for UpdateData {
        const VARIANT_INDEX: usize = 1;
    }

    impl crate::event_map::EventVariant for DeleteData {
        const VARIANT_INDEX: usize = 2;
    }

    // Test event enum (will need UnsafeEvent impl)
    #[derive(Debug, Clone)]
    enum TestEvent {
        Create(CreateData),
        Update(UpdateData),
        Delete(DeleteData),
    }

    // Manual implementation for testing (derive macro will generate this)
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

        fn inner_as_any(&self) -> &dyn std::any::Any {
            match self {
                TestEvent::Create(data) => data,
                TestEvent::Update(data) => data,
                TestEvent::Delete(data) => data,
            }
        }

        fn inner_type_id(&self) -> std::any::TypeId {
            match self {
                TestEvent::Create(_) => std::any::TypeId::of::<CreateData>(),
                TestEvent::Update(_) => std::any::TypeId::of::<UpdateData>(),
                TestEvent::Delete(_) => std::any::TypeId::of::<DeleteData>(),
            }
        }

        unsafe fn call_handler_with_data<M, C>(
            self,
            handler_ptr: *const (),
            model: &mut M,
        ) -> crate::dispatch::Dispatch<Self, C> {
            match self {
                TestEvent::Create(data) => {
                    type HandlerType<M, C> = fn(CreateData, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                    handler(data, model)
                }
                TestEvent::Update(data) => {
                    type HandlerType<M, C> = fn(UpdateData, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                    handler(data, model)
                }
                TestEvent::Delete(data) => {
                    type HandlerType<M, C> = fn(DeleteData, &mut M) -> crate::dispatch::Dispatch<TestEvent, C>;
                    let handler = mem::transmute::<*const (), HandlerType<M, C>>(handler_ptr);
                    handler(data, model)
                }
            }
        }
    }

    // Test command and model
    #[derive(Debug, Clone)]
    enum TestCommand {
        Log(String),
    }

    #[derive(Default)]
    struct TestModel {
        items: Vec<String>,
        counter: usize,
    }

    // Handler functions that receive concrete types
    fn handle_create(data: CreateData, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
        model.items.push(data.name);
        model.counter += 1;
        Dispatch::none()
    }

    fn handle_update(data: UpdateData, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
        if let Some(item) = model.items.get_mut(data.id as usize) {
            *item = data.name;
        }
        Dispatch::none()
    }

    fn handle_delete(data: DeleteData, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
        if (data.id as usize) < model.items.len() {
            model.items.remove(data.id as usize);
        }
        model.counter = model.counter.saturating_sub(1);
        Dispatch::command(TestCommand::Log(format!("Deleted {}", data.id)))
    }

    #[test]
    fn test_unsafe_event_map() {
        let mut map = UnsafeEventMap::<TestEvent, TestModel, TestCommand>::new();
        let mut model = TestModel::default();

        unsafe {
            // Register handlers with concrete types
            map.register::<CreateData>(0, handle_create);
            map.register::<UpdateData>(1, handle_update);
            map.register::<DeleteData>(2, handle_delete);
        }

        // Test create event
        let event = TestEvent::Create(CreateData { name: "Alice".to_string() });
        unsafe {
            let result = map.dispatch(event, &mut model);
            assert!(result.events.is_empty());
            assert!(result.commands.is_empty());
        }

        assert_eq!(model.items.len(), 1);
        assert_eq!(model.items[0], "Alice");
        assert_eq!(model.counter, 1);

        // Test update event
        let event = TestEvent::Update(UpdateData {
            id: 0,
            name: "Alice Smith".to_string()
        });
        unsafe {
            map.dispatch(event, &mut model);
        }

        assert_eq!(model.items[0], "Alice Smith");

        // Test delete event
        let event = TestEvent::Delete(DeleteData { id: 0 });
        unsafe {
            let result = map.dispatch(event, &mut model);
            assert!(result.events.is_empty());
            assert_eq!(result.commands.len(), 1);
            assert!(matches!(result.commands[0], TestCommand::Log(_)));
        }

        assert_eq!(model.items.len(), 0);
        assert_eq!(model.counter, 0);
    }

    #[test]
    fn test_unsafe_event_map_builder() {
        let map = unsafe {
            UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
                .on::<CreateData>(handle_create)
                .on::<DeleteData>(handle_delete)
                .build()
        };

        let mut model = TestModel::default();

        // Test registered handler
        let event = TestEvent::Create(CreateData { name: "Bob".to_string() });
        unsafe {
            map.dispatch(event, &mut model);
        }
        assert_eq!(model.items.len(), 1);

        // Test unregistered handler (should return Dispatch::none)
        let event = TestEvent::Update(UpdateData { id: 0, name: "Updated".to_string() });
        unsafe {
            let result = map.dispatch(event, &mut model);
            assert!(result.events.is_empty());
            assert!(result.commands.is_empty());
        }

        // Model should be unchanged since handler wasn't registered
        assert_eq!(model.items.len(), 1);
        assert_eq!(model.items[0], "Bob");
    }

    #[test]
    fn test_has_handler() {
        let mut map = UnsafeEventMap::<TestEvent, TestModel, TestCommand>::new();

        assert!(!map.has_handler(0));
        assert!(!map.has_handler(1));
        assert!(!map.has_handler(2));
        assert_eq!(map.handler_count(), 0);

        unsafe {
            map.register::<CreateData>(CreateData::VARIANT_INDEX, handle_create);
            map.register::<DeleteData>(DeleteData::VARIANT_INDEX, handle_delete);
        }

        assert!(map.has_handler(0));
        assert!(!map.has_handler(1));
        assert!(map.has_handler(2));
        assert_eq!(map.handler_count(), 2);
    }

    #[test]
    fn test_empty_unsafe_event_map() {
        let map = UnsafeEventMap::<TestEvent, TestModel, TestCommand>::new();
        let mut model = TestModel::default();

        // All events should return none dispatch with no handlers registered
        let events = vec![
            TestEvent::Create(CreateData { name: "Alice".to_string() }),
            TestEvent::Update(UpdateData { id: 0, name: "Bob".to_string() }),
            TestEvent::Delete(DeleteData { id: 0 }),
        ];

        for event in events {
            let result = unsafe { map.dispatch(event, &mut model) };
            assert!(result.events.is_empty());
            assert!(result.commands.is_empty());
        }

        // Model should be unchanged
        assert_eq!(model.items.len(), 0);
        assert_eq!(model.counter, 0);
        assert_eq!(map.handler_count(), 0);
    }

    #[test]
    fn test_unsafe_event_map_bounds_safety() {
        let map = UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new();
        let map = unsafe { map.build() };

        // Test that out-of-bounds checks work correctly
        assert!(!map.has_handler(100)); // Way out of bounds
        assert_eq!(map.handler_count(), 0);

        // Test all valid indices
        for i in 0..TestEvent::LENGTH {
            assert!(!map.has_handler(i));
        }
    }

    #[test]
    fn test_unsafe_event_map_with_complex_dispatch() {
        fn handle_create_complex(data: CreateData, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
            model.items.push(data.name.clone());
            model.counter += 1;

            // Generate cascade event and command
            Dispatch::new(
                vec![TestEvent::Update(UpdateData { id: 0, name: format!("Updated: {}", data.name) })],
                vec![TestCommand::Log(format!("Created complex: {}", data.name))]
            )
        }

        fn handle_update_complex(data: UpdateData, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
            if let Some(item) = model.items.get_mut(data.id as usize) {
                *item = data.name.clone();
                model.counter += 1;
            }
            Dispatch::command(TestCommand::Log(format!("Updated complex: {}", data.name)))
        }

        let map = unsafe {
            UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
                .on::<CreateData>(handle_create_complex)
                .on::<UpdateData>(handle_update_complex)
                .build()
        };

        let mut model = TestModel::default();

        let result = unsafe {
            map.dispatch(TestEvent::Create(CreateData {
                name: "Alice".to_string()
            }), &mut model)
        };

        // Verify complex dispatch result
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.commands.len(), 1);
        assert_eq!(model.items.len(), 1);
        assert_eq!(model.counter, 1);

        // Process the generated event
        if let Some(update_event) = result.events.into_iter().next() {
            let update_result = unsafe { map.dispatch(update_event, &mut model) };
            assert_eq!(update_result.commands.len(), 1);
            assert_eq!(model.counter, 2);
            assert_eq!(model.items[0], "Updated: Alice");
        }
    }

    #[test]
    fn test_unsafe_event_map_handler_pointer_safety() {
        // Test that handler pointers are correctly stored and retrieved
        fn test_handler(data: CreateData, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
            model.items.push(format!("Test: {}", data.name));
            Dispatch::none()
        }

        let mut map = UnsafeEventMap::<TestEvent, TestModel, TestCommand>::new();

        unsafe {
            map.register::<CreateData>(CreateData::VARIANT_INDEX, test_handler);
        }

        assert!(map.has_handler(CreateData::VARIANT_INDEX));
        assert_eq!(map.handler_count(), 1);

        // Test that the handler actually works
        let mut model = TestModel::default();
        let result = unsafe {
            map.dispatch(TestEvent::Create(CreateData {
                name: "Test".to_string()
            }), &mut model)
        };

        assert_eq!(model.items.len(), 1);
        assert_eq!(model.items[0], "Test: Test");
        assert!(result.events.is_empty());
        assert!(result.commands.is_empty());
    }

    #[test]
    fn test_unsafe_event_map_variant_index_validation() {
        // Test that EventVariant constants are correct for our test types
        assert_eq!(CreateData::VARIANT_INDEX, 0);
        assert_eq!(UpdateData::VARIANT_INDEX, 1);
        assert_eq!(DeleteData::VARIANT_INDEX, 2);

        // Test that variant indices match Event implementation
        let events = vec![
            TestEvent::Create(CreateData { name: "test".to_string() }),
            TestEvent::Update(UpdateData { id: 1, name: "test".to_string() }),
            TestEvent::Delete(DeleteData { id: 2 }),
        ];

        let expected_indices = [0, 1, 2];
        for (event, expected_index) in events.iter().zip(expected_indices.iter()) {
            assert_eq!(event.variant_index(), *expected_index);
        }
    }

    #[test]
    fn test_unsafe_event_map_null_pointer_handling() {
        let map = UnsafeEventMap::<TestEvent, TestModel, TestCommand>::new();

        // Verify all handlers start as null
        for i in 0..TestEvent::LENGTH {
            assert!(!map.has_handler(i));
        }

        let mut model = TestModel::default();

        // Dispatching to null handlers should return Dispatch::none()
        let result = unsafe {
            map.dispatch(TestEvent::Create(CreateData {
                name: "test".to_string()
            }), &mut model)
        };

        assert!(result.events.is_empty());
        assert!(result.commands.is_empty());
        assert_eq!(model.items.len(), 0);
        assert_eq!(model.counter, 0);
    }

    #[test]
    fn test_unsafe_event_map_multiple_handlers() {
        // Test registering handlers in different patterns
        let map1 = unsafe {
            UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
                .on::<CreateData>(handle_create)
                .on::<UpdateData>(handle_update)
                .on::<DeleteData>(handle_delete)
                .build()
        };

        let map2 = unsafe {
            UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
                .on::<DeleteData>(handle_delete)  // Different order
                .on::<CreateData>(handle_create)
                .on::<UpdateData>(handle_update)
                .build()
        };

        // Both should have same handler count and coverage
        assert_eq!(map1.handler_count(), map2.handler_count());
        for i in 0..TestEvent::LENGTH {
            assert_eq!(map1.has_handler(i), map2.has_handler(i));
        }

        // Both should produce identical results
        let mut model1 = TestModel::default();
        let mut model2 = TestModel::default();

        let test_event = TestEvent::Create(CreateData { name: "test".to_string() });

        let result1 = unsafe { map1.dispatch(test_event.clone(), &mut model1) };
        let result2 = unsafe { map2.dispatch(test_event, &mut model2) };

        assert_eq!(model1.items, model2.items);
        assert_eq!(model1.counter, model2.counter);
        assert_eq!(result1.events.len(), result2.events.len());
        assert_eq!(result1.commands.len(), result2.commands.len());
    }

    #[test]
    fn test_unsafe_event_map_memory_layout() {
        // Test that the UnsafeEventMap has expected memory characteristics
        let map = UnsafeEventMap::<TestEvent, TestModel, TestCommand>::new();

        // Should be able to check all possible handler slots
        for i in 0..TestEvent::LENGTH {
            assert!(!map.has_handler(i));
        }

        // Should handle edge case of checking exactly at length boundary
        assert!(!map.has_handler(TestEvent::LENGTH));

        // Handler count should be accurate
        assert_eq!(map.handler_count(), 0);

        // After registering one handler
        let mut map = map;
        unsafe {
            map.register::<CreateData>(CreateData::VARIANT_INDEX, handle_create);
        }

        assert_eq!(map.handler_count(), 1);
        assert!(map.has_handler(CreateData::VARIANT_INDEX));

        for i in 0..TestEvent::LENGTH {
            if i == CreateData::VARIANT_INDEX {
                assert!(map.has_handler(i));
            } else {
                assert!(!map.has_handler(i));
            }
        }
    }
}
