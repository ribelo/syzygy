//! Comparison tests for EventMap and UnsafeEventMap implementations
//!
//! This test suite verifies that both EventMap and UnsafeEventMap produce
//! identical results for the same event sequences, ensuring correctness
//! across both safe and unsafe implementations.

use syzygy::dispatch::Dispatch;
use syzygy::event_map::{EventMap, EventMapBuilder, EventVariant};
use syzygy::unsafe_event_map::{UnsafeEventMap, UnsafeEventMapBuilder};
use syzygy_macros::Event;

// Common test data types - identical to benchmark types
#[derive(Clone, Debug, PartialEq)]
struct Event1 { id: u32 }

#[derive(Clone, Debug, PartialEq)]
struct Event2 { value: u64 }

#[derive(Clone, Debug, PartialEq)]
struct Event3 { flag: bool }

#[derive(Clone, Debug, PartialEq)]
struct Event4 { count: u16 }

#[derive(Clone, Debug, PartialEq)]
struct Event5 { index: usize }

// Test event enum with Event derive
#[derive(Clone, Debug, Event)]
enum TestEvent {
    Event1(Event1),
    Event2(Event2),
    Event3(Event3),
    Event4(Event4),
    Event5(Event5),
}

// Test command enum
#[derive(Debug, Clone, PartialEq)]
enum TestCommand {
    Log(String),
    Increment(u32),
    Flag(bool),
}

// Test model that tracks all mutations
#[derive(Default, Debug, Clone, PartialEq)]
struct TestModel {
    counter: u64,
    processed_events: u64,
    last_id: Option<u32>,
    last_value: Option<u64>,
    flag_toggles: u32,
    event_counts: [u32; 5], // Track count per event type
}

// Handler functions for EventMap (take inner types directly)
fn handle_event1_safe(data: Event1, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    model.processed_events += 1;
    model.counter += data.id as u64;
    model.last_id = Some(data.id);
    model.event_counts[0] += 1;
    
    if data.id % 10 == 0 {
        Dispatch::command(TestCommand::Log(format!("Processed Event1 with id {}", data.id)))
    } else {
        Dispatch::none()
    }
}

fn handle_event2_safe(data: Event2, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    model.processed_events += 1;
    model.counter += data.value;
    model.last_value = Some(data.value);
    model.event_counts[1] += 1;
    
    if data.value > 100 {
        Dispatch::event(TestEvent::Event3(Event3 { flag: true }))
    } else {
        Dispatch::none()
    }
}

fn handle_event3_safe(data: Event3, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    model.processed_events += 1;
    model.counter += if data.flag { 1 } else { 0 };
    model.event_counts[2] += 1;
    
    if data.flag {
        model.flag_toggles += 1;
        Dispatch::command(TestCommand::Flag(true))
    } else {
        Dispatch::none()
    }
}

fn handle_event4_safe(data: Event4, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    model.processed_events += 1;
    model.counter += data.count as u64;
    model.event_counts[3] += 1;
    
    Dispatch::command(TestCommand::Increment(data.count as u32))
}

fn handle_event5_safe(data: Event5, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    model.processed_events += 1;
    model.counter += data.index as u64;
    model.event_counts[4] += 1;
    
    if data.index % 2 == 0 {
        Dispatch::new(
            vec![TestEvent::Event1(Event1 { id: data.index as u32 })],
            vec![TestCommand::Log(format!("Generated from Event5: {}", data.index))]
        )
    } else {
        Dispatch::none()
    }
}

// Handler functions for UnsafeEventMap (take inner types directly)
fn handle_event1_unsafe(data: Event1, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    // Identical logic to safe version
    handle_event1_safe(data, model)
}

fn handle_event2_unsafe(data: Event2, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    // Identical logic to safe version
    handle_event2_safe(data, model)
}

fn handle_event3_unsafe(data: Event3, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    // Identical logic to safe version
    handle_event3_safe(data, model)
}

fn handle_event4_unsafe(data: Event4, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    // Identical logic to safe version
    handle_event4_safe(data, model)
}

fn handle_event5_unsafe(data: Event5, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    // Identical logic to safe version
    handle_event5_safe(data, model)
}

// Test data generation
fn generate_test_events(count: usize, pattern: &str) -> Vec<TestEvent> {
    let mut events = Vec::with_capacity(count);
    let mut rng_state: u64 = 42; // Fixed seed for reproducibility

    for i in 0..count {
        let event_type = match pattern {
            "sequential" => i % 5,
            "random" => {
                rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
                (rng_state % 5) as usize
            },
            "concentrated" => if i < count / 2 { 0 } else { (i % 4) + 1 }, // Mostly Event1, then others
            _ => panic!("Unknown pattern: {}", pattern),
        };

        let event = match event_type {
            0 => TestEvent::Event1(Event1 { id: i as u32 }),
            1 => TestEvent::Event2(Event2 { value: (i * 10) as u64 }),
            2 => TestEvent::Event3(Event3 { flag: (i % 2) == 0 }),
            3 => TestEvent::Event4(Event4 { count: (i % 1000) as u16 }),
            4 => TestEvent::Event5(Event5 { index: i }),
            _ => unreachable!(),
        };

        events.push(event);
    }

    events
}

// Setup functions for both implementations
fn setup_safe_event_map() -> EventMap<TestEvent, TestCommand, TestModel> {
    EventMapBuilder::<TestEvent, TestCommand, TestModel>::new()
        .on(handle_event1_safe)
        .on(handle_event2_safe)
        .on(handle_event3_safe)
        .on(handle_event4_safe)
        .on(handle_event5_safe)
        .build()
}

fn setup_unsafe_event_map() -> UnsafeEventMap<TestEvent, TestModel, TestCommand> {
    unsafe {
        UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
            .on::<Event1>(handle_event1_unsafe)
            .on::<Event2>(handle_event2_unsafe)
            .on::<Event3>(handle_event3_unsafe)
            .on::<Event4>(handle_event4_unsafe)
            .on::<Event5>(handle_event5_unsafe)
            .build()
    }
}

// Helper to collect all dispatch results
#[derive(Debug, Clone, PartialEq)]
struct DispatchResults {
    final_model: TestModel,
    total_events: usize,
    total_commands: usize,
    event_types: Vec<String>,
    command_types: Vec<String>,
}

fn collect_dispatch_results<F>(events: Vec<TestEvent>, mut dispatcher: F) -> DispatchResults
where
    F: FnMut(TestEvent, &mut TestModel) -> Dispatch<TestEvent, TestCommand>,
{
    let mut model = TestModel::default();
    let mut all_events = Vec::new();
    let mut all_commands = Vec::new();
    
    // Process each event and collect results
    for event in events {
        let result = dispatcher(event, &mut model);
        
        // Collect generated events and commands
        for e in result.events {
            all_events.push(format!("{:?}", e));
        }
        for c in result.commands {
            all_commands.push(format!("{:?}", c));
        }
    }
    
    DispatchResults {
        final_model: model,
        total_events: all_events.len(),
        total_commands: all_commands.len(),
        event_types: all_events,
        command_types: all_commands,
    }
}

#[test]
fn test_identical_results_sequential_pattern() {
    let events = generate_test_events(100, "sequential");
    
    let safe_map = setup_safe_event_map();
    let unsafe_map = setup_unsafe_event_map();
    
    // Test safe implementation
    let safe_results = collect_dispatch_results(events.clone(), |event, model| {
        safe_map.dispatch(event, model)
    });
    
    // Test unsafe implementation
    let unsafe_results = collect_dispatch_results(events, |event, model| {
        unsafe { unsafe_map.dispatch(event, model) }
    });
    
    // Verify identical results
    assert_eq!(safe_results.final_model, unsafe_results.final_model, 
               "Final model states should be identical");
    assert_eq!(safe_results.total_events, unsafe_results.total_events,
               "Total generated events should be identical");
    assert_eq!(safe_results.total_commands, unsafe_results.total_commands,
               "Total generated commands should be identical");
    assert_eq!(safe_results.event_types, unsafe_results.event_types,
               "Generated event types should be identical");
    assert_eq!(safe_results.command_types, unsafe_results.command_types,
               "Generated command types should be identical");
}

#[test]
fn test_identical_results_random_pattern() {
    let events = generate_test_events(100, "random");
    
    let safe_map = setup_safe_event_map();
    let unsafe_map = setup_unsafe_event_map();
    
    let safe_results = collect_dispatch_results(events.clone(), |event, model| {
        safe_map.dispatch(event, model)
    });
    
    let unsafe_results = collect_dispatch_results(events, |event, model| {
        unsafe { unsafe_map.dispatch(event, model) }
    });
    
    assert_eq!(safe_results.final_model, unsafe_results.final_model);
    assert_eq!(safe_results.total_events, unsafe_results.total_events);
    assert_eq!(safe_results.total_commands, unsafe_results.total_commands);
    assert_eq!(safe_results.event_types, unsafe_results.event_types);
    assert_eq!(safe_results.command_types, unsafe_results.command_types);
}

#[test]
fn test_identical_results_concentrated_pattern() {
    let events = generate_test_events(50, "concentrated");
    
    let safe_map = setup_safe_event_map();
    let unsafe_map = setup_unsafe_event_map();
    
    let safe_results = collect_dispatch_results(events.clone(), |event, model| {
        safe_map.dispatch(event, model)
    });
    
    let unsafe_results = collect_dispatch_results(events, |event, model| {
        unsafe { unsafe_map.dispatch(event, model) }
    });
    
    assert_eq!(safe_results.final_model, unsafe_results.final_model);
    assert_eq!(safe_results.total_events, unsafe_results.total_events);
    assert_eq!(safe_results.total_commands, unsafe_results.total_commands);
}

#[test]
fn test_single_event_dispatch() {
    let safe_map = setup_safe_event_map();
    let unsafe_map = setup_unsafe_event_map();
    
    // Test each event type individually
    let test_events = vec![
        TestEvent::Event1(Event1 { id: 42 }),
        TestEvent::Event2(Event2 { value: 150 }), // Should trigger Event3 generation
        TestEvent::Event3(Event3 { flag: true }),
        TestEvent::Event4(Event4 { count: 99 }),
        TestEvent::Event5(Event5 { index: 20 }), // Should trigger Event1 generation
    ];
    
    for event in test_events {
        let mut safe_model = TestModel::default();
        let mut unsafe_model = TestModel::default();
        
        let safe_result = safe_map.dispatch(event.clone(), &mut safe_model);
        let unsafe_result = unsafe { unsafe_map.dispatch(event, &mut unsafe_model) };
        
        assert_eq!(safe_model, unsafe_model, "Models should be identical after single event");
        assert_eq!(safe_result.events.len(), unsafe_result.events.len(), "Generated events count should match");
        assert_eq!(safe_result.commands.len(), unsafe_result.commands.len(), "Generated commands count should match");
        
        // Compare events and commands (they should be structurally equivalent)
        for (safe_event, unsafe_event) in safe_result.events.iter().zip(unsafe_result.events.iter()) {
            assert_eq!(format!("{:?}", safe_event), format!("{:?}", unsafe_event));
        }
        
        for (safe_cmd, unsafe_cmd) in safe_result.commands.iter().zip(unsafe_result.commands.iter()) {
            assert_eq!(format!("{:?}", safe_cmd), format!("{:?}", unsafe_cmd));
        }
    }
}

#[test]
fn test_unregistered_handlers() {
    // Create maps with only some handlers registered
    let partial_safe_map = EventMapBuilder::<TestEvent, TestCommand, TestModel>::new()
        .on(handle_event1_safe)
        .on(handle_event3_safe)
        // Skip Event2, Event4, Event5
        .build();
    
    let partial_unsafe_map = unsafe {
        UnsafeEventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
            .on::<Event1>(handle_event1_unsafe)
            .on::<Event3>(handle_event3_unsafe)
            // Skip Event2, Event4, Event5
            .build()
    };
    
    let test_events = vec![
        TestEvent::Event1(Event1 { id: 1 }),    // Registered
        TestEvent::Event2(Event2 { value: 2 }), // Not registered
        TestEvent::Event3(Event3 { flag: true }), // Registered  
        TestEvent::Event4(Event4 { count: 4 }), // Not registered
        TestEvent::Event5(Event5 { index: 5 }), // Not registered
    ];
    
    let safe_results = collect_dispatch_results(test_events.clone(), |event, model| {
        partial_safe_map.dispatch(event, model)
    });
    
    let unsafe_results = collect_dispatch_results(test_events, |event, model| {
        unsafe { partial_unsafe_map.dispatch(event, model) }
    });
    
    // Should get identical results even with partial registration
    assert_eq!(safe_results.final_model, unsafe_results.final_model);
    assert_eq!(safe_results.total_events, unsafe_results.total_events);
    assert_eq!(safe_results.total_commands, unsafe_results.total_commands);
    
    // Only Event1 and Event3 should have been processed
    assert_eq!(safe_results.final_model.event_counts[0], 1); // Event1
    assert_eq!(safe_results.final_model.event_counts[1], 0); // Event2 (unregistered)
    assert_eq!(safe_results.final_model.event_counts[2], 1); // Event3
    assert_eq!(safe_results.final_model.event_counts[3], 0); // Event4 (unregistered)
    assert_eq!(safe_results.final_model.event_counts[4], 0); // Event5 (unregistered)
}

#[test]
fn test_model_state_consistency() {
    let events = generate_test_events(20, "sequential");
    
    let safe_map = setup_safe_event_map();
    let unsafe_map = setup_unsafe_event_map();
    
    let mut safe_model = TestModel::default();
    let mut unsafe_model = TestModel::default();
    
    // Process events one by one and check consistency at each step
    for (i, event) in events.into_iter().enumerate() {
        let safe_result = safe_map.dispatch(event.clone(), &mut safe_model);
        let unsafe_result = unsafe { unsafe_map.dispatch(event, &mut unsafe_model) };
        
        assert_eq!(safe_model, unsafe_model, 
                   "Models should be identical at step {}", i);
        assert_eq!(safe_result.events.len(), unsafe_result.events.len(), 
                   "Event generation should be identical at step {}", i);
        assert_eq!(safe_result.commands.len(), unsafe_result.commands.len(), 
                   "Command generation should be identical at step {}", i);
    }
}

#[test]
fn test_large_scale_consistency() {
    let events = generate_test_events(1000, "random");
    
    let safe_map = setup_safe_event_map();
    let unsafe_map = setup_unsafe_event_map();
    
    let safe_results = collect_dispatch_results(events.clone(), |event, model| {
        safe_map.dispatch(event, model)
    });
    
    let unsafe_results = collect_dispatch_results(events, |event, model| {
        unsafe { unsafe_map.dispatch(event, model) }
    });
    
    // Verify all aspects match for large scale processing
    assert_eq!(safe_results.final_model, unsafe_results.final_model);
    assert_eq!(safe_results.total_events, unsafe_results.total_events);
    assert_eq!(safe_results.total_commands, unsafe_results.total_commands);
    assert_eq!(safe_results.event_types, unsafe_results.event_types);
    assert_eq!(safe_results.command_types, unsafe_results.command_types);
    
    // Verify meaningful processing occurred
    assert!(safe_results.final_model.processed_events > 0);
    assert!(safe_results.final_model.counter > 0);
    assert!(safe_results.final_model.event_counts.iter().sum::<u32>() > 0);
}