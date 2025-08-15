//! UnsafeEventMap correctness tests with safety validation
//!
//! This test suite validates the UnsafeEventMap implementation's correctness,
//! safety invariants, and performance characteristics while ensuring proper
//! unsafe code usage patterns.

use syzygy::dispatch::Dispatch;
use syzygy::event_map::{Event, EventVariant};
use syzygy::unsafe_event_map::{UnsafeEventMap, UnsafeEventMapBuilder};
use syzygy_macros::Event;

// Performance-focused test data types
#[derive(Clone, Debug, PartialEq)]
struct FastEvent1 { id: u32 }

#[derive(Clone, Debug, PartialEq)]
struct FastEvent2 { value: u64 }

#[derive(Clone, Debug, PartialEq)]
struct FastEvent3 { flag: bool }

#[derive(Clone, Debug, PartialEq)]
struct FastEvent4 { data: [u8; 8] }

#[derive(Clone, Debug, PartialEq)]
struct FastEvent5 { timestamp: u64, counter: u32 }

// Test event enum with Event derive
#[derive(Clone, Debug, Event)]
enum FastEvent {
    Fast1(FastEvent1),
    Fast2(FastEvent2),
    Fast3(FastEvent3),
    Fast4(FastEvent4),
    Fast5(FastEvent5),
}

// Minimal command enum for performance testing
#[derive(Debug, Clone, PartialEq)]
enum FastCommand {
    Increment,
    Log(u64),
    Trigger(u32),
}

// Performance-focused model
#[derive(Default, Debug, Clone, PartialEq)]
struct FastModel {
    counter: u64,
    processed: u64,
    events_by_type: [u32; 5],
    last_timestamp: u64,
    flags: u32,
}

// Handler functions optimized for performance
fn handle_fast1(data: FastEvent1, model: &mut FastModel) -> Dispatch<FastEvent, FastCommand> {
    model.processed += 1;
    model.counter += data.id as u64;
    model.events_by_type[0] += 1;
    
    if data.id % 100 == 0 {
        Dispatch::command(FastCommand::Log(data.id as u64))
    } else {
        Dispatch::none()
    }
}

fn handle_fast2(data: FastEvent2, model: &mut FastModel) -> Dispatch<FastEvent, FastCommand> {
    model.processed += 1;
    model.counter += data.value;
    model.events_by_type[1] += 1;
    
    if data.value > 1000 {
        Dispatch::new(
            vec![FastEvent::Fast3(FastEvent3 { flag: true })],
            vec![FastCommand::Trigger(data.value as u32)]
        )
    } else {
        Dispatch::none()
    }
}

fn handle_fast3(data: FastEvent3, model: &mut FastModel) -> Dispatch<FastEvent, FastCommand> {
    model.processed += 1;
    model.events_by_type[2] += 1;
    
    if data.flag {
        model.flags += 1;
        Dispatch::command(FastCommand::Increment)
    } else {
        model.flags = model.flags.saturating_sub(1);
        Dispatch::none()
    }
}

fn handle_fast4(data: FastEvent4, model: &mut FastModel) -> Dispatch<FastEvent, FastCommand> {
    model.processed += 1;
    model.events_by_type[3] += 1;
    
    // Sum bytes for some computation
    let sum: u32 = data.data.iter().map(|&b| b as u32).sum();
    model.counter += sum as u64;
    
    Dispatch::none()
}

fn handle_fast5(data: FastEvent5, model: &mut FastModel) -> Dispatch<FastEvent, FastCommand> {
    model.processed += 1;
    model.events_by_type[4] += 1;
    model.last_timestamp = data.timestamp;
    model.counter += data.counter as u64;
    
    if data.timestamp % 2 == 0 {
        Dispatch::new(
            vec![FastEvent::Fast1(FastEvent1 { id: data.counter })],
            vec![FastCommand::Log(data.timestamp)]
        )
    } else {
        Dispatch::none()
    }
}

#[test]
fn test_unsafe_event_map_basic_functionality() {
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent2>(handle_fast2)
            .on::<FastEvent3>(handle_fast3)
            .on::<FastEvent4>(handle_fast4)
            .on::<FastEvent5>(handle_fast5)
            .build()
    };
    
    let mut model = FastModel::default();
    
    // Test basic dispatch
    let event = FastEvent::Fast1(FastEvent1 { id: 42 });
    let result = unsafe { event_map.dispatch(event, &mut model) };
    
    assert_eq!(model.processed, 1);
    assert_eq!(model.counter, 42);
    assert_eq!(model.events_by_type[0], 1);
    assert!(result.events.is_empty());
    assert!(result.commands.is_empty());
}

#[test]
fn test_unsafe_simplified_api() {
    // Verify that simplified API works without manual indices
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)  // No manual index needed
            .on::<FastEvent3>(handle_fast3)  // Can skip FastEvent2
            .on::<FastEvent5>(handle_fast5)  // Can register in any order
            .build()
    };
    
    let mut model = FastModel::default();
    
    // Test registered handlers
    unsafe {
        event_map.dispatch(FastEvent::Fast1(FastEvent1 { id: 10 }), &mut model);
        event_map.dispatch(FastEvent::Fast3(FastEvent3 { flag: true }), &mut model);
        event_map.dispatch(FastEvent::Fast5(FastEvent5 { timestamp: 123, counter: 5 }), &mut model);
    }
    
    assert_eq!(model.processed, 3);
    assert_eq!(model.events_by_type[0], 1); // FastEvent1
    assert_eq!(model.events_by_type[1], 0); // FastEvent2 (not registered)
    assert_eq!(model.events_by_type[2], 1); // FastEvent3
    assert_eq!(model.events_by_type[3], 0); // FastEvent4 (not registered)
    assert_eq!(model.events_by_type[4], 1); // FastEvent5
    
    // Test unregistered handler
    let result = unsafe {
        event_map.dispatch(FastEvent::Fast2(FastEvent2 { value: 999 }), &mut model)
    };
    
    assert!(result.events.is_empty());
    assert!(result.commands.is_empty());
    assert_eq!(model.processed, 3); // Should not have incremented
}

#[test]
fn test_unsafe_event_map_variant_indexing() {
    // Verify automatic variant indexing works correctly
    assert_eq!(FastEvent1::VARIANT_INDEX, 0);
    assert_eq!(FastEvent2::VARIANT_INDEX, 1);
    assert_eq!(FastEvent3::VARIANT_INDEX, 2);
    assert_eq!(FastEvent4::VARIANT_INDEX, 3);
    assert_eq!(FastEvent5::VARIANT_INDEX, 4);
    
    // Test that builders use correct indices
    let map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent3>(handle_fast3)  // Should map to index 2
            .on::<FastEvent1>(handle_fast1)  // Should map to index 0
            .build()
    };
    
    assert!(map.has_handler(0)); // FastEvent1
    assert!(!map.has_handler(1)); // FastEvent2 (not registered)
    assert!(map.has_handler(2)); // FastEvent3
    assert!(!map.has_handler(3)); // FastEvent4 (not registered)
    assert!(!map.has_handler(4)); // FastEvent5 (not registered)
}

#[test]
fn test_unsafe_event_map_cascade_events() {
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent2>(handle_fast2)
            .on::<FastEvent3>(handle_fast3)
            .on::<FastEvent5>(handle_fast5)
            .build()
    };
    
    let mut model = FastModel::default();
    
    // Test event that generates cascade events
    let result = unsafe {
        event_map.dispatch(FastEvent::Fast2(FastEvent2 { value: 2000 }), &mut model)
    };
    
    assert_eq!(model.processed, 1);
    assert_eq!(result.events.len(), 1); // Should generate FastEvent3
    assert_eq!(result.commands.len(), 1); // Should generate Trigger command
    
    // Verify the generated event is correct
    if let FastEvent::Fast3(fast3) = &result.events[0] {
        assert!(fast3.flag);
    } else {
        panic!("Expected FastEvent3");
    }
    
    // Test event that generates both events and commands
    let result = unsafe {
        event_map.dispatch(FastEvent::Fast5(FastEvent5 { timestamp: 100, counter: 42 }), &mut model)
    };
    
    assert_eq!(result.events.len(), 1); // Should generate FastEvent1
    assert_eq!(result.commands.len(), 1); // Should generate Log command
    
    if let FastEvent::Fast1(fast1) = &result.events[0] {
        assert_eq!(fast1.id, 42);
    } else {
        panic!("Expected FastEvent1");
    }
}

#[test]
fn test_unsafe_event_map_performance_patterns() {
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent2>(handle_fast2)
            .on::<FastEvent3>(handle_fast3)
            .on::<FastEvent4>(handle_fast4)
            .on::<FastEvent5>(handle_fast5)
            .build()
    };
    
    let mut model = FastModel::default();
    
    // Test high-frequency pattern
    let events: Vec<_> = (0..1000).map(|i| {
        match i % 5 {
            0 => FastEvent::Fast1(FastEvent1 { id: i as u32 }),
            1 => FastEvent::Fast2(FastEvent2 { value: i as u64 }),
            2 => FastEvent::Fast3(FastEvent3 { flag: (i % 2) == 0 }),
            3 => FastEvent::Fast4(FastEvent4 { data: [i as u8; 8] }),
            4 => FastEvent::Fast5(FastEvent5 { timestamp: i as u64, counter: i as u32 }),
            _ => unreachable!(),
        }
    }).collect();
    
    let mut total_events = 0;
    let mut total_commands = 0;
    
    for event in events {
        let result = unsafe { event_map.dispatch(event, &mut model) };
        total_events += result.events.len();
        total_commands += result.commands.len();
    }
    
    // Verify processing
    assert_eq!(model.processed, 1000);
    assert!(model.counter > 0);
    assert!(total_events > 0); // Some events should generate cascades
    assert!(total_commands > 0); // Some events should generate commands
    
    // Verify event type distribution
    let total_by_type: u32 = model.events_by_type.iter().sum();
    assert_eq!(total_by_type, 1000);
    
    // Each type should have roughly 200 events (1000 / 5)
    for &count in &model.events_by_type {
        assert!(count >= 190 && count <= 210, "Event distribution: {:?}", model.events_by_type);
    }
}

#[test]
fn test_unsafe_event_map_error_conditions() {
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            // Only register one handler to test unregistered behavior
            .build()
    };
    
    let mut model = FastModel::default();
    
    // Test registered handler works
    let result = unsafe {
        event_map.dispatch(FastEvent::Fast1(FastEvent1 { id: 42 }), &mut model)
    };
    assert_eq!(model.processed, 1);
    
    // Test unregistered handlers return none and don't modify model
    let original_model = model.clone();
    let result = unsafe {
        event_map.dispatch(FastEvent::Fast2(FastEvent2 { value: 100 }), &mut model)
    };
    
    assert!(result.events.is_empty());
    assert!(result.commands.is_empty());
    assert_eq!(model.processed, original_model.processed); // Should be unchanged
}

#[test]
fn test_unsafe_event_map_memory_safety() {
    // Test that multiple maps can coexist safely
    let map1 = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent2>(handle_fast2)
            .build()
    };
    
    let map2 = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent3>(handle_fast3)
            .on::<FastEvent4>(handle_fast4)
            .build()
    };
    
    let mut model1 = FastModel::default();
    let mut model2 = FastModel::default();
    
    // Use maps independently
    unsafe {
        map1.dispatch(FastEvent::Fast1(FastEvent1 { id: 10 }), &mut model1);
        map2.dispatch(FastEvent::Fast3(FastEvent3 { flag: true }), &mut model2);
    }
    
    assert_eq!(model1.processed, 1);
    assert_eq!(model1.events_by_type[0], 1);
    assert_eq!(model1.events_by_type[2], 0);
    
    assert_eq!(model2.processed, 1);
    assert_eq!(model2.events_by_type[0], 0);
    assert_eq!(model2.events_by_type[2], 1);
}

#[test]
fn test_unsafe_event_map_handler_count() {
    let mut builder = UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new();
    let map = unsafe {
        builder
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent3>(handle_fast3)
            .on::<FastEvent5>(handle_fast5)
            .build()
    };
    
    assert_eq!(map.handler_count(), 3);
    assert!(map.has_handler(0)); // FastEvent1
    assert!(!map.has_handler(1)); // FastEvent2
    assert!(map.has_handler(2)); // FastEvent3
    assert!(!map.has_handler(3)); // FastEvent4
    assert!(map.has_handler(4)); // FastEvent5
}

#[test]
fn test_unsafe_event_map_builder_pattern() {
    // Test that builder can be constructed in different ways
    let map1 = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent2>(handle_fast2)
            .on::<FastEvent3>(handle_fast3)
            .build()
    };
    
    let map2 = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent3>(handle_fast3) // Different order
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent2>(handle_fast2)
            .build()
    };
    
    // Both maps should have identical handler registration
    assert_eq!(map1.handler_count(), map2.handler_count());
    for i in 0..5 {
        assert_eq!(map1.has_handler(i), map2.has_handler(i));
    }
    
    // Both maps should produce identical results
    let mut model1 = FastModel::default();
    let mut model2 = FastModel::default();
    
    let test_event = FastEvent::Fast2(FastEvent2 { value: 1500 });
    
    let result1 = unsafe { map1.dispatch(test_event.clone(), &mut model1) };
    let result2 = unsafe { map2.dispatch(test_event, &mut model2) };
    
    assert_eq!(model1, model2);
    assert_eq!(result1.events.len(), result2.events.len());
    assert_eq!(result1.commands.len(), result2.commands.len());
}

#[test]
fn test_unsafe_event_map_zero_copy_semantics() {
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent4>(handle_fast4) // Handler that processes byte array
            .build()
    };
    
    let mut model = FastModel::default();
    
    // Create event with specific data pattern
    let data = [1, 2, 3, 4, 5, 6, 7, 8];
    let event = FastEvent::Fast4(FastEvent4 { data });
    
    let result = unsafe { event_map.dispatch(event, &mut model) };
    
    // Verify that the data was processed correctly (sum of bytes = 36)
    assert_eq!(model.counter, 36);
    assert_eq!(model.processed, 1);
    assert!(result.events.is_empty());
    assert!(result.commands.is_empty());
}

#[test]
fn test_unsafe_event_map_bounds_checking() {
    // This test verifies that the map correctly handles variant indices
    let event_map = unsafe {
        UnsafeEventMapBuilder::<FastEvent, FastModel, FastCommand>::new()
            .on::<FastEvent1>(handle_fast1)
            .on::<FastEvent5>(handle_fast5)
            .build()
    };
    
    // Test with all possible event variants
    let events = vec![
        FastEvent::Fast1(FastEvent1 { id: 1 }),
        FastEvent::Fast2(FastEvent2 { value: 2 }),
        FastEvent::Fast3(FastEvent3 { flag: true }),
        FastEvent::Fast4(FastEvent4 { data: [0; 8] }),
        FastEvent::Fast5(FastEvent5 { timestamp: 5, counter: 5 }),
    ];
    
    let mut model = FastModel::default();
    
    for event in events {
        // Should not panic or crash, even for unregistered handlers
        let _result = unsafe { event_map.dispatch(event, &mut model) };
    }
    
    // Only FastEvent1 and FastEvent5 should have been processed
    assert_eq!(model.events_by_type[0], 1); // FastEvent1
    assert_eq!(model.events_by_type[1], 0); // FastEvent2 (unregistered)
    assert_eq!(model.events_by_type[2], 0); // FastEvent3 (unregistered)
    assert_eq!(model.events_by_type[3], 0); // FastEvent4 (unregistered)
    assert_eq!(model.events_by_type[4], 1); // FastEvent5
}