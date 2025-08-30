//! Tests for the iterator-style Effects builder API
//!
//! These tests verify that the Effects builder provides a clean, chainable API
//! for coordinating effects without semantic confusion or magic behavior.

use syzygy::prelude::*;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
enum TestEvent {
    AppReady,
    DataReceived,
    ProcessComplete,
    FirstWins,
}

#[derive(Debug, Clone, PartialEq)]
enum TestEffect {
    LoadConfig,
    LoadUser,
    Mirror1,
    Mirror2,
    Step1,
    Step2,
    Step3,
}

#[test]
fn test_effects_parallel_with_barrier() {
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::LoadConfig, TestEffect::LoadUser])
        .parallel()
        .barrier(TestEvent::AppReady);

    // Should produce a Group with parallel mode and barrier
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Group { effects, mode, barrier, timeout_per } => {
            assert_eq!(effects, &vec![TestEffect::LoadConfig, TestEffect::LoadUser]);
            assert_eq!(*mode, syzygy::command::GroupMode::Parallel);
            assert_eq!(barrier, &Some(TestEvent::AppReady));
            assert_eq!(timeout_per, &None);
        }
        _ => panic!("Expected Group step, got {:?}", outputs[0])
    }
}

#[test]
fn test_effects_parallel_fire_and_forget() {
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::LoadConfig, TestEffect::LoadUser])
        .parallel()
        .spawn();

    // Should produce a Group with parallel mode and no barrier
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Group { effects, mode, barrier, timeout_per } => {
            assert_eq!(effects, &vec![TestEffect::LoadConfig, TestEffect::LoadUser]);
            assert_eq!(*mode, syzygy::command::GroupMode::Parallel);
            assert_eq!(barrier, &None);
            assert_eq!(timeout_per, &None);
        }
        _ => panic!("Expected Group step")
    }
}

#[test]
fn test_effects_race_with_barrier() {
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::Mirror1, TestEffect::Mirror2])
        .race()
        .barrier(TestEvent::FirstWins);

    // Should produce a Group with race mode and barrier
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Group { effects, mode, barrier, timeout_per } => {
            assert_eq!(effects, &vec![TestEffect::Mirror1, TestEffect::Mirror2]);
            assert_eq!(*mode, syzygy::command::GroupMode::Race);
            assert_eq!(barrier, &Some(TestEvent::FirstWins));
            assert_eq!(timeout_per, &None);
        }
        _ => panic!("Expected Group step")
    }
}

#[test]
fn test_effects_race_fire_and_forget() {
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::Mirror1, TestEffect::Mirror2])
        .race()
        .spawn();

    // Should produce a Group with race mode and no barrier
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Group { effects, mode, barrier, timeout_per } => {
            assert_eq!(effects, &vec![TestEffect::Mirror1, TestEffect::Mirror2]);
            assert_eq!(*mode, syzygy::command::GroupMode::Race);
            assert_eq!(barrier, &None);
            assert_eq!(timeout_per, &None);
        }
        _ => panic!("Expected Group step")
    }
}

#[test]
fn test_effects_sequence_with_barrier() {
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::Step1, TestEffect::Step2, TestEffect::Step3])
        .sequence()
        .barrier(TestEvent::ProcessComplete);

    // Should produce a Batch followed by Event
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 2);

    match &outputs[0] {
        CommandStep::Batch(effects) => {
            assert_eq!(effects, &vec![TestEffect::Step1, TestEffect::Step2, TestEffect::Step3]);
        }
        _ => panic!("Expected Batch step")
    }

    match &outputs[1] {
        CommandStep::Event(event) => {
            assert_eq!(event, &TestEvent::ProcessComplete);
        }
        _ => panic!("Expected Event step")
    }
}

#[test]
fn test_effects_sequence_fire_and_forget() {
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::Step1, TestEffect::Step2, TestEffect::Step3])
        .sequence()
        .spawn();

    // Should produce just a Batch
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Batch(effects) => {
            assert_eq!(effects, &vec![TestEffect::Step1, TestEffect::Step2, TestEffect::Step3]);
        }
        _ => panic!("Expected Batch step")
    }
}

#[test]
fn test_effects_with_timeout() {
    let timeout = Duration::from_secs(5);
    let cmd = Effects::<TestEvent, TestEffect>::new([TestEffect::LoadConfig, TestEffect::LoadUser])
        .parallel()
        .timeout_per(timeout)
        .barrier(TestEvent::AppReady);

    // Should produce a Group with timeout_per set
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Group { effects, mode, barrier, timeout_per } => {
            assert_eq!(effects, &vec![TestEffect::LoadConfig, TestEffect::LoadUser]);
            assert_eq!(*mode, syzygy::command::GroupMode::Parallel);
            assert_eq!(barrier, &Some(TestEvent::AppReady));
            assert_eq!(timeout_per, &Some(timeout));
        }
        _ => panic!("Expected Group step with timeout")
    }
}

#[test]
fn test_effects_chaining_feels_like_iterator() {
    // This test verifies the API feels natural like iterator chaining
    let _parallel_cmd = Effects::new([TestEffect::LoadConfig, TestEffect::LoadUser])
        .parallel()                                    // Like .map()
        .timeout_per(Duration::from_secs(5))          // Like .filter() 
        .barrier(TestEvent::AppReady);                // Like .collect()

    let _race_cmd: Command<TestEvent, TestEffect> = Effects::new([TestEffect::Mirror1, TestEffect::Mirror2])
        .race()                                       // Mode switch
        .timeout_per(Duration::from_secs(1))          // Modifier
        .spawn();                                     // Terminal like .for_each()

    let _sequence_cmd = Effects::new([TestEffect::Step1, TestEffect::Step2])
        .sequence()                                   // Mode switch
        .stop_on_error(false)                         // Modifier
        .barrier(TestEvent::ProcessComplete);         // Terminal

    // If this compiles, the API feels natural
}

#[test]
fn test_empty_effects() {
    // Empty parallel effects with barrier should just emit the event
    let cmd = Effects::<TestEvent, TestEffect>::new(Vec::<TestEffect>::new())
        .parallel()
        .barrier(TestEvent::AppReady);

    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Event(event) => {
            assert_eq!(event, &TestEvent::AppReady);
        }
        _ => panic!("Expected Event step for empty effects with barrier")
    }

    // Empty parallel effects without barrier should be Command::none()
    let cmd = Effects::<TestEvent, TestEffect>::new(Vec::<TestEffect>::new())
        .parallel()
        .spawn();

    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 0); // Command::none() has no outputs

    // Empty sequence effects with barrier should just emit the event
    let cmd = Effects::<TestEvent, TestEffect>::new(Vec::<TestEffect>::new())
        .sequence()
        .barrier(TestEvent::ProcessComplete);

    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);

    match &outputs[0] {
        CommandStep::Event(event) => {
            assert_eq!(event, &TestEvent::ProcessComplete);
        }
        _ => panic!("Expected Event step for empty sequence with barrier")
    }
}

// Note: Backwards compatibility test removed since Command::parallel() was deprecated and removed
// in favor of the unified Effects::new().parallel().spawn() API

#[test]
fn test_sequential_configuration() {
    let cmd = Effects::new([TestEffect::Step1, TestEffect::Step2])
        .sequence()
        .stop_on_error(false)  // Continue on error
        .barrier(TestEvent::ProcessComplete);

    // Sequential builder configuration is tested - if it compiles, it works
    // The actual stop_on_error behavior would be tested in Shell integration tests
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 2); // Batch + Event
}

#[test]
fn test_label_configuration() {
    let cmd = Effects::new([TestEffect::LoadConfig])
        .parallel()
        .label("config-loading")  // For tracing/debugging
        .barrier(TestEvent::AppReady);

    // Label configuration is tested - the actual tracing would be tested elsewhere
    let outputs = cmd.outputs();
    assert_eq!(outputs.len(), 1);
}

/// Integration test showing real-world usage patterns
#[test]
fn test_real_world_patterns() {
    // Pattern 1: Load multiple resources in parallel, wait for all
    let _bootstrap = Effects::new([
        TestEffect::LoadConfig,
        TestEffect::LoadUser,
    ])
    .parallel()
    .timeout_per(Duration::from_secs(10))
    .barrier(TestEvent::AppReady);

    // Pattern 2: Try multiple mirrors, first one wins
    let _failover = Effects::new([
        TestEffect::Mirror1,
        TestEffect::Mirror2,
    ])
    .race()
    .timeout_per(Duration::from_secs(5))
    .barrier(TestEvent::DataReceived);

    // Pattern 3: Sequential workflow with completion notification
    let _workflow = Effects::new([
        TestEffect::Step1,
        TestEffect::Step2,
        TestEffect::Step3,
    ])
    .sequence()
    .stop_on_error(true)
    .barrier(TestEvent::ProcessComplete);

    // Pattern 4: Fire-and-forget cleanup
    let _cleanup: Command<TestEvent, TestEffect> = Effects::new([TestEffect::LoadConfig])
        .parallel()
        .spawn();

    // If these compile, the API supports real-world patterns
}