//! Test for SYZ-018: Deterministic Behavior
//!
//! These tests verify that the Syzygy system produces deterministic,
//! reproducible behavior for the same event sequences.

use syzygy::prelude::*;
use syzygy::resource::Resources;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq)]
struct CounterModel {
    count: i32,
    operations: Vec<String>,
    total_events: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CounterEvent {
    Increment(i32),
    Decrement(i32),
    Multiply(i32),
    Reset,
    LogOperation(String),
}

#[derive(Debug, Clone)]
enum CounterCommand {
    SaveState(i32),
    NotifyExternal(String),
}

fn create_counter_syzygy() -> (Syzygy<CounterModel, CounterEvent, CounterCommand, Resources>, SyzygyHandle<CounterEvent>) {
    Syzygy::builder()
        .model(CounterModel::default())
        .event_handler(|event, model| {
            model.total_events += 1;
            
            match event {
                CounterEvent::Increment(n) => {
                    model.count += n;
                    model.operations.push(format!("inc:{}", n));
                    if model.count > 100 {
                        Dispatch::command(CounterCommand::SaveState(model.count))
                    } else {
                        Dispatch::none()
                    }
                }
                CounterEvent::Decrement(n) => {
                    model.count -= n;
                    model.operations.push(format!("dec:{}", n));
                    Dispatch::none()
                }
                CounterEvent::Multiply(n) => {
                    model.count *= n;
                    model.operations.push(format!("mul:{}", n));
                    if n == 0 {
                        // Multiplication by zero triggers a reset event
                        Dispatch::event(CounterEvent::Reset)
                    } else {
                        Dispatch::none()
                    }
                }
                CounterEvent::Reset => {
                    model.count = 0;
                    model.operations.clear();
                    model.operations.push("reset".to_string());
                    Dispatch::command(CounterCommand::NotifyExternal("Reset completed".to_string()))
                }
                CounterEvent::LogOperation(op) => {
                    model.operations.push(op);
                    Dispatch::none()
                }
            }
        })
        .build()
}

#[test]
fn test_syz_018_basic_deterministic_behavior() {
    // Test that the same event sequence produces identical results
    let events = vec![
        TimestampedEvent {
            event: CounterEvent::Increment(5),
            timestamp_ms: 0,
        },
        TimestampedEvent {
            event: CounterEvent::Increment(3),
            timestamp_ms: 10,
        },
        TimestampedEvent {
            event: CounterEvent::Decrement(2),
            timestamp_ms: 20,
        },
        TimestampedEvent {
            event: CounterEvent::Multiply(2),
            timestamp_ms: 30,
        },
    ];

    // Run determinism test with 10 iterations
    let result = TestUtils::assert_deterministic(create_counter_syzygy, events, 10);
    
    assert!(result.is_ok(), "System should be deterministic: {:?}", result);
}

#[test]
fn test_syz_018_complex_event_chains() {
    // Test determinism with events that trigger other events
    let events = vec![
        TimestampedEvent {
            event: CounterEvent::Increment(10),
            timestamp_ms: 0,
        },
        TimestampedEvent {
            event: CounterEvent::Multiply(0), // This triggers Reset
            timestamp_ms: 10,
        },
        TimestampedEvent {
            event: CounterEvent::Increment(42),
            timestamp_ms: 20,
        },
        TimestampedEvent {
            event: CounterEvent::LogOperation("test".to_string()),
            timestamp_ms: 30,
        },
    ];

    let result = TestUtils::assert_deterministic(create_counter_syzygy, events, 5);
    assert!(result.is_ok(), "Complex event chains should be deterministic: {:?}", result);
}

#[test]
fn test_syz_018_state_consistency() {
    // Test that repeated execution of the same scenario produces consistent results
    let test_scenario = |handle: &SyzygyHandle<CounterEvent>| {
        handle.dispatch(CounterEvent::Increment(7)).unwrap();
        handle.dispatch(CounterEvent::Increment(13)).unwrap();
        handle.dispatch(CounterEvent::Decrement(5)).unwrap();
        handle.dispatch(CounterEvent::Multiply(3)).unwrap();
        handle.dispatch(CounterEvent::LogOperation("final".to_string())).unwrap();
    };

    let result = TestUtils::verify_consistency(create_counter_syzygy, test_scenario, 8);
    assert!(result.is_ok(), "State should be consistent across runs: {:?}", result);
}

#[test]
fn test_syz_018_record_and_replay() {
    // Test full record and replay cycle
    let (mut syzygy, handle) = create_counter_syzygy();

    // First, test without recording to verify the logic works
    handle.dispatch(CounterEvent::Increment(15)).unwrap();
    handle.dispatch(CounterEvent::Multiply(4)).unwrap();
    handle.dispatch(CounterEvent::Decrement(10)).unwrap();
    handle.dispatch(CounterEvent::LogOperation("recorded".to_string())).unwrap();
    
    // Process all events
    while syzygy.has_pending_events() {
        syzygy.process_events();
    }
    
    // Verify the final state first
    println!("Direct processing result: count={}, operations={:?}", 
             syzygy.model().count, syzygy.model().operations);
    assert_eq!(syzygy.model().count, 50); // (15 * 4) - 10 = 50
    assert_eq!(syzygy.model().operations, vec![
        "inc:15", "mul:4", "dec:10", "recorded"
    ]);

    // Now test recording - create a fresh system
    let (mut fresh_syzygy, fresh_handle) = create_counter_syzygy();
    let scenario = TestUtils::record_scenario(&mut fresh_syzygy, &fresh_handle, |h| {
        h.dispatch(CounterEvent::Increment(15)).unwrap();
        h.dispatch(CounterEvent::Multiply(4)).unwrap();
        h.dispatch(CounterEvent::Decrement(10)).unwrap();
        h.dispatch(CounterEvent::LogOperation("recorded".to_string())).unwrap();
    });

    println!("Recorded scenario: count={}, operations={:?}, events={}", 
             scenario.final_model.count, scenario.final_model.operations, scenario.recorded_events.len());

    // Verify the recorded scenario matches
    assert_eq!(scenario.final_model.count, 50);
    assert_eq!(scenario.final_model.operations, vec![
        "inc:15", "mul:4", "dec:10", "recorded"
    ]);

    // Now test replay with manually created events
    let manual_events = vec![
        TimestampedEvent {
            event: CounterEvent::Increment(15),
            timestamp_ms: 0,
        },
        TimestampedEvent {
            event: CounterEvent::Multiply(4),
            timestamp_ms: 10,
        },
        TimestampedEvent {
            event: CounterEvent::Decrement(10),
            timestamp_ms: 20,
        },
        TimestampedEvent {
            event: CounterEvent::LogOperation("recorded".to_string()),
            timestamp_ms: 30,
        },
    ];

    let (replay_syzygy, replay_handle) = create_counter_syzygy();
    let config = TestConfig::default();
    
    let replay_result = TestUtils::replay_scenario(
        replay_syzygy, 
        &replay_handle, 
        manual_events,
        config
    ).unwrap();

    println!("Replay result: count={}, operations={:?}", 
             replay_result.final_model.count, replay_result.final_model.operations);

    // Verify replay produces identical results
    assert_eq!(replay_result.final_model.count, 50);
    assert_eq!(replay_result.final_model.operations, vec![
        "inc:15", "mul:4", "dec:10", "recorded"
    ]);
}

#[test]
fn test_syz_018_event_serialization() {
    // Test that events can be serialized and deserialized for persistent deterministic testing
    let mut recorder = EventRecorder::new();
    recorder.start_recording();
    
    recorder.record_event(CounterEvent::Increment(42));
    recorder.record_event(CounterEvent::Multiply(2));
    recorder.record_event(CounterEvent::LogOperation("serialized".to_string()));
    
    recorder.stop_recording();

    // Serialize to JSON
    let json = recorder.to_json().unwrap();
    assert!(json.contains("Increment"));
    assert!(json.contains("42"));
    assert!(json.contains("Multiply"));
    assert!(json.contains("serialized"));

    // Deserialize and verify
    let loaded_events = EventRecorder::<CounterEvent>::from_json(&json).unwrap();
    assert_eq!(loaded_events.len(), 3);

    // Use deserialized events for replay
    let result = TestUtils::assert_deterministic(create_counter_syzygy, loaded_events, 3);
    assert!(result.is_ok(), "Serialized events should be deterministic: {:?}", result);
}

#[test]
fn test_syz_018_timing_modes() {
    // Test that different timing modes produce the same final state
    let events = vec![
        TimestampedEvent {
            event: CounterEvent::Increment(20),
            timestamp_ms: 0,
        },
        TimestampedEvent {
            event: CounterEvent::Decrement(5),
            timestamp_ms: 100,
        },
        TimestampedEvent {
            event: CounterEvent::Multiply(3),
            timestamp_ms: 200,
        },
    ];

    // Test immediate timing
    let (syzygy1, handle1) = create_counter_syzygy();
    let config1 = TestConfig {
        step_mode: true,
        ..Default::default()
    };
    let result1 = TestUtils::replay_scenario(syzygy1, &handle1, events.clone(), config1).unwrap();

    // Test accelerated timing
    let (syzygy2, handle2) = create_counter_syzygy();
    let config2 = TestConfig {
        step_mode: false,
        ..Default::default()
    };
    let result2 = TestUtils::replay_scenario(syzygy2, &handle2, events, config2).unwrap();

    // Both should produce the same final state
    assert_eq!(result1.final_model, result2.final_model);
    assert_eq!(result1.final_model.count, 45); // (20 - 5) * 3 = 45
}

#[test]
fn test_syz_018_step_by_step_determinism() {
    // Test that step-by-step processing maintains determinism
    let events = vec![
        TimestampedEvent {
            event: CounterEvent::Increment(1),
            timestamp_ms: 0,
        },
        TimestampedEvent {
            event: CounterEvent::Increment(1),
            timestamp_ms: 1,
        },
        TimestampedEvent {
            event: CounterEvent::Increment(1),
            timestamp_ms: 2,
        },
        TimestampedEvent {
            event: CounterEvent::Multiply(10),
            timestamp_ms: 3,
        },
    ];

    let step_config = TestUtils::step_config();
    
    // Run multiple step-by-step replays
    let mut results = Vec::new();
    for _ in 0..5 {
        let (syzygy, handle) = create_counter_syzygy();
        let result = TestUtils::replay_scenario(syzygy, &handle, events.clone(), step_config.clone()).unwrap();
        results.push(result);
    }

    // All results should be identical
    let first_result = &results[0];
    for result in &results[1..] {
        assert_eq!(result.final_model, first_result.final_model);
        assert_eq!(result.events_processed, first_result.events_processed);
    }

    // Verify the expected final state
    assert_eq!(first_result.final_model.count, 30); // (1 + 1 + 1) * 10 = 30
}

#[test]
fn test_syz_018_large_event_sequence() {
    // Test determinism with a larger event sequence
    let mut events = Vec::new();
    
    // Generate a predictable sequence of events
    for i in 0..50 {
        events.push(TimestampedEvent {
            event: if i % 5 == 0 {
                CounterEvent::Reset
            } else if i % 3 == 0 {
                CounterEvent::Multiply(2)
            } else if i % 2 == 0 {
                CounterEvent::Increment(i as i32)
            } else {
                CounterEvent::Decrement(1)
            },
            timestamp_ms: i * 10,
        });
    }

    let result = TestUtils::assert_deterministic(create_counter_syzygy, events, 3);
    assert!(result.is_ok(), "Large event sequence should be deterministic: {:?}", result);
}

/// Test utility to verify that SYZ-018 requirement is fully met
#[test]
fn test_syz_018_requirement_verification() {
    // This test serves as comprehensive verification that SYZ-018 is implemented
    println!("Testing SYZ-018: Deterministic Behavior");
    
    // 1. Event recording works
    let mut recorder = EventRecorder::new();
    recorder.start_recording();
    recorder.record_event(CounterEvent::Increment(1));
    assert!(recorder.is_recording());
    assert_eq!(recorder.event_count(), 1);
    println!("✓ Event recording capability");

    // 2. Event replay works
    let events = vec![TimestampedEvent {
        event: CounterEvent::Increment(42),
        timestamp_ms: 0,
    }];
    let mut replayer = EventReplayer::new(events);
    replayer.start();
    match replayer.next_event() {
        ReplayResult::Event(CounterEvent::Increment(42)) => {},
        _ => panic!("Replay failed"),
    }
    println!("✓ Event replay capability");

    // 3. Deterministic testing works
    let simple_events = vec![TimestampedEvent {
        event: CounterEvent::Increment(5),
        timestamp_ms: 0,
    }];
    let result = TestUtils::assert_deterministic(create_counter_syzygy, simple_events, 3);
    assert!(result.is_ok());
    println!("✓ Deterministic testing utilities");

    // 4. Full integration works
    let (mut syzygy, handle) = create_counter_syzygy();
    let scenario = TestUtils::record_scenario(&mut syzygy, &handle, |h| {
        h.dispatch(CounterEvent::Increment(10)).unwrap();
    });
    assert_eq!(scenario.final_model.count, 10);
    println!("✓ Full record/replay integration");

    println!("SYZ-018: Deterministic Behavior - REQUIREMENT MET ✓");
}