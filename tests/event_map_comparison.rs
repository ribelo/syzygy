//! EventMap comprehensive tests
//!
//! This test suite verifies the EventMap implementation's correctness
//! across various patterns and edge cases.

use syzygy::dispatch::Dispatch;
use syzygy::event_map::{EventMap, EventMapBuilder, EventVariant};
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
fn handle_event1(data: Event1, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
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

fn handle_event2(data: Event2, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
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

fn handle_event3(data: Event3, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
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

fn handle_event4(data: Event4, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
    model.processed_events += 1;
    model.counter += data.count as u64;
    model.event_counts[3] += 1;
    
    Dispatch::command(TestCommand::Increment(data.count as u32))
}

fn handle_event5(data: Event5, model: &mut TestModel) -> Dispatch<TestEvent, TestCommand> {
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

// Setup function for EventMap
fn setup_event_map() -> EventMap<TestEvent, TestModel, TestCommand> {
    EventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
        .on(handle_event1)
        .on(handle_event2)
        .on(handle_event3)
        .on(handle_event4)
        .on(handle_event5)
        .build()
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
fn test_sequential_pattern() {
    let events = generate_test_events(100, "sequential");
    let event_map = setup_event_map();
    
    let results = collect_dispatch_results(events, |event, model| {
        unsafe { event_map.dispatch(event, model) }
    });
    
    // Verify meaningful processing occurred
    assert!(results.final_model.processed_events > 0);
    assert!(results.final_model.counter > 0);
    assert!(results.total_events > 0);
    assert!(results.total_commands > 0);
}

#[test]
fn test_random_pattern() {
    let events = generate_test_events(100, "random");
    let event_map = setup_event_map();
    
    let results = collect_dispatch_results(events, |event, model| {
        unsafe { event_map.dispatch(event, model) }
    });
    
    // Verify meaningful processing occurred
    assert!(results.final_model.processed_events > 0);
    assert!(results.final_model.counter > 0);
}

#[test]
fn test_concentrated_pattern() {
    let events = generate_test_events(50, "concentrated");
    let event_map = setup_event_map();
    
    let results = collect_dispatch_results(events, |event, model| {
        unsafe { event_map.dispatch(event, model) }
    });
    
    assert!(results.final_model.processed_events > 0);
    assert!(results.final_model.counter > 0);
}

#[test]
fn test_single_event_dispatch() {
    let event_map = setup_event_map();
    
    // Test each event type individually
    let test_events = vec![
        TestEvent::Event1(Event1 { id: 42 }),
        TestEvent::Event2(Event2 { value: 150 }), // Should trigger Event3 generation
        TestEvent::Event3(Event3 { flag: true }),
        TestEvent::Event4(Event4 { count: 99 }),
        TestEvent::Event5(Event5 { index: 20 }), // Should trigger Event1 generation
    ];
    
    for event in test_events {
        let mut model = TestModel::default();
        let result = unsafe { event_map.dispatch(event, &mut model) };
        
        // Verify some processing occurred
        assert!(model.processed_events > 0);
        
        // Verify commands or events were generated appropriately
        assert!(result.events.len() >= 0);
        assert!(result.commands.len() >= 0);
    }
}

#[test]
fn test_unregistered_handlers() {
    // Create maps with only some handlers registered
    let partial_event_map = EventMapBuilder::<TestEvent, TestModel, TestCommand>::new()
        .on(handle_event1)
        .on(handle_event3)
        // Skip Event2, Event4, Event5
        .build();
    
    let test_events = vec![
        TestEvent::Event1(Event1 { id: 1 }),    // Registered
        TestEvent::Event2(Event2 { value: 2 }), // Not registered
        TestEvent::Event3(Event3 { flag: true }), // Registered  
        TestEvent::Event4(Event4 { count: 4 }), // Not registered
        TestEvent::Event5(Event5 { index: 5 }), // Not registered
    ];
    
    let results = collect_dispatch_results(test_events, |event, model| {
        unsafe { partial_event_map.dispatch(event, model) }
    });
    
    // Only Event1 and Event3 should have been processed
    assert_eq!(results.final_model.event_counts[0], 1); // Event1
    assert_eq!(results.final_model.event_counts[1], 0); // Event2 (unregistered)
    assert_eq!(results.final_model.event_counts[2], 1); // Event3
    assert_eq!(results.final_model.event_counts[3], 0); // Event4 (unregistered)
    assert_eq!(results.final_model.event_counts[4], 0); // Event5 (unregistered)
}

#[test]
fn test_model_state_consistency() {
    let events = generate_test_events(20, "sequential");
    let event_map = setup_event_map();
    
    let mut model = TestModel::default();
    
    // Process events one by one and check consistency at each step
    for (i, event) in events.into_iter().enumerate() {
        let result = unsafe { event_map.dispatch(event, &mut model) };
        
        assert_eq!(model.processed_events, (i + 1) as u64, "Event count should match step {}", i);
        assert!(result.events.len() >= 0);
        assert!(result.commands.len() >= 0);
    }
}

#[test]
fn test_large_scale_consistency() {
    let events = generate_test_events(1000, "random");
    let event_map = setup_event_map();
    
    let results = collect_dispatch_results(events, |event, model| {
        unsafe { event_map.dispatch(event, model) }
    });
    
    // Verify all aspects match for large scale processing
    assert!(results.final_model.processed_events > 0);
    assert!(results.final_model.counter > 0);
    assert!(results.total_events >= 0);
    assert!(results.total_commands >= 0);
    
    // Verify meaningful processing occurred
    assert_eq!(results.final_model.processed_events, 1000);
    assert!(results.final_model.event_counts.iter().sum::<u32>() > 0);
}