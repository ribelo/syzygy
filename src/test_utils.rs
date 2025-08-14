//! Test utilities for deterministic testing
//!
//! This module provides high-level utilities for deterministic testing
//! of Syzygy systems using event recording and replay.

use crate::{
    syzygy::Syzygy,
    recorder::{EventRecorder, TimestampedEvent},
    replay::{EventReplayer, ReplayResult, TimingMode},
    handle::SyzygyHandle,
};
use std::fmt::Debug;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

/// Test scenario result containing final state and recorded events
#[derive(Debug, Clone)]
pub struct TestScenario<M, E> {
    pub final_model: M,
    pub recorded_events: Vec<TimestampedEvent<E>>,
    pub events_processed: u64,
}

/// Configuration for deterministic testing
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Maximum time to wait for event processing to complete
    pub timeout: Duration,
    /// Whether to record events during the test
    pub record_events: bool,
    /// Step-by-step processing (process one event at a time)
    pub step_mode: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            record_events: true,
            step_mode: false,
        }
    }
}


/// Test utilities for deterministic Syzygy testing
pub struct TestUtils;

impl TestUtils {
    /// Record a test scenario by executing a test function and capturing events
    pub fn record_scenario<M, E, C, R, F>(
        syzygy: &mut Syzygy<M, E, C, R>,
        handle: &SyzygyHandle<E>,
        test_fn: F,
    ) -> TestScenario<M, E>
    where
        M: Clone + Debug,
        E: Clone + Debug + Send,
        C: Clone + Debug + Send,
        F: FnOnce(&SyzygyHandle<E>),
    {
        let mut recorded_events = Vec::new();
        let start_time = Instant::now();

        // Create a custom handle that records events
        let event_sender = handle.event_sender().clone();
        let recording_handle = SyzygyHandle::new(event_sender);

        // Execute the test function and capture the events being dispatched
        let mut event_count = 0;
        test_fn(&recording_handle);

        // Process events and record them
        while syzygy.has_pending_events() {
            // Capture any events that were dispatched before processing
            while let Ok(event) = syzygy.event_rx.try_recv() {
                let timestamp_ms = start_time.elapsed().as_millis() as u64;
                recorded_events.push(TimestampedEvent {
                    event: event.clone(),
                    timestamp_ms,
                });
                
                // Put the event back for processing
                let _ = syzygy.event_tx.send(event);
                event_count += 1;
                break; // Process one at a time
            }
            
            if event_count > 0 {
                syzygy.process_events();
                event_count = 0;
            } else {
                break;
            }
        }

        TestScenario {
            final_model: syzygy.model().clone(),
            recorded_events,
            events_processed: syzygy.total_events_processed(),
        }
    }

    /// Replay a scenario from recorded events and return the final state
    pub fn replay_scenario<M, E, C, R>(
        mut syzygy: Syzygy<M, E, C, R>,
        handle: &SyzygyHandle<E>,
        events: Vec<TimestampedEvent<E>>,
        config: TestConfig,
    ) -> Result<TestScenario<M, E>, ReplayError>
    where
        M: Clone + Debug,
        E: Clone + Debug + Send,
        C: Clone + Debug + Send,
    {
        let mut replayer = EventReplayer::new(events.clone())
            .with_timing_mode(if config.step_mode {
                TimingMode::Immediate
            } else {
                TimingMode::Accelerated(10.0) // 10x speed for faster tests
            });

        replayer.start();

        let start_time = std::time::Instant::now();

        loop {
            // Check timeout
            if start_time.elapsed() > config.timeout {
                return Err(ReplayError::Timeout);
            }

            // Get next event from replayer
            match replayer.next_event() {
                ReplayResult::Event(event) => {
                    // Dispatch the event
                    handle.dispatch(event).map_err(|_| ReplayError::ChannelClosed)?;
                    
                    // Process events
                    if config.step_mode {
                        // Process exactly one event
                        syzygy.process_events();
                    } else {
                        // Process all pending events
                        while syzygy.has_pending_events() {
                            syzygy.process_events();
                        }
                    }
                }
                ReplayResult::Wait(duration) => {
                    // Wait for the specified duration
                    std::thread::sleep(duration);
                }
                ReplayResult::Complete => {
                    // Replay is complete, process any remaining events
                    while syzygy.has_pending_events() {
                        syzygy.process_events();
                    }
                    break;
                }
            }
        }

        Ok(TestScenario {
            final_model: syzygy.model().clone(),
            recorded_events: events,
            events_processed: syzygy.total_events_processed(),
        })
    }

    /// Assert that replaying the same events produces the same result (deterministic behavior)
    pub fn assert_deterministic<M, E, C, R>(
        create_syzygy: fn() -> (Syzygy<M, E, C, R>, SyzygyHandle<E>),
        events: Vec<TimestampedEvent<E>>,
        iterations: usize,
    ) -> Result<(), DeterminismError>
    where
        M: Clone + Debug + PartialEq,
        E: Clone + Debug + Send,
        C: Clone + Debug + Send,
    {
        if iterations < 2 {
            return Err(DeterminismError::InsufficientIterations);
        }

        let config = TestConfig {
            record_events: false,
            step_mode: true, // Use step mode for precise determinism
            ..Default::default()
        };

        let mut results = Vec::new();

        for i in 0..iterations {
            let (syzygy, handle) = create_syzygy();
            let result = Self::replay_scenario(syzygy, &handle, events.clone(), config.clone())
                .map_err(|e| DeterminismError::ReplayFailed(i, e))?;
            results.push(result);
        }

        // Compare all results to the first one
        let first_result = &results[0];
        for (i, result) in results.iter().enumerate().skip(1) {
            if result.final_model != first_result.final_model {
                return Err(DeterminismError::StateMismatch {
                    iteration: i,
                    expected: format!("{:?}", first_result.final_model),
                    actual: format!("{:?}", result.final_model),
                });
            }

            if result.events_processed != first_result.events_processed {
                return Err(DeterminismError::EventCountMismatch {
                    iteration: i,
                    expected: first_result.events_processed,
                    actual: result.events_processed,
                });
            }
        }

        Ok(())
    }

    /// Create a test configuration for step-by-step processing
    pub fn step_config() -> TestConfig {
        TestConfig {
            step_mode: true,
            ..Default::default()
        }
    }

    /// Create a test configuration for fast replay testing
    pub fn fast_config() -> TestConfig {
        TestConfig {
            timeout: Duration::from_millis(100),
            record_events: false,
            step_mode: false,
        }
    }

    /// Verify that a Syzygy system produces consistent results for the same inputs
    pub fn verify_consistency<M, E, C, R, F>(
        create_syzygy: fn() -> (Syzygy<M, E, C, R>, SyzygyHandle<E>),
        test_fn: F,
        iterations: usize,
    ) -> Result<(), ConsistencyError>
    where
        M: Clone + Debug + PartialEq,
        E: Clone + Debug + Send,
        C: Clone + Debug + Send,
        F: Fn(&SyzygyHandle<E>) + Clone,
    {
        if iterations < 2 {
            return Err(ConsistencyError::InsufficientIterations);
        }

        let mut results = Vec::new();

        for i in 0..iterations {
            let (mut syzygy, handle) = create_syzygy();
            let scenario = Self::record_scenario(&mut syzygy, &handle, test_fn.clone());
            results.push(scenario);
        }

        // Compare all results to the first one
        let first_result = &results[0];
        for (i, result) in results.iter().enumerate().skip(1) {
            if result.final_model != first_result.final_model {
                return Err(ConsistencyError::StateMismatch {
                    iteration: i,
                    expected: format!("{:?}", first_result.final_model),
                    actual: format!("{:?}", result.final_model),
                });
            }
        }

        Ok(())
    }
}

/// Errors that can occur during replay
#[derive(Debug, Clone)]
pub enum ReplayError {
    Timeout,
    ChannelClosed,
}

/// Errors that can occur during determinism testing
#[derive(Debug, Clone)]
pub enum DeterminismError {
    InsufficientIterations,
    ReplayFailed(usize, ReplayError),
    StateMismatch {
        iteration: usize,
        expected: String,
        actual: String,
    },
    EventCountMismatch {
        iteration: usize,
        expected: u64,
        actual: u64,
    },
}

/// Errors that can occur during consistency testing
#[derive(Debug, Clone)]
pub enum ConsistencyError {
    InsufficientIterations,
    StateMismatch {
        iteration: usize,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Timeout => write!(f, "Replay timed out"),
            ReplayError::ChannelClosed => write!(f, "Event channel closed during replay"),
        }
    }
}

impl std::fmt::Display for DeterminismError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeterminismError::InsufficientIterations => {
                write!(f, "Need at least 2 iterations for determinism testing")
            }
            DeterminismError::ReplayFailed(iteration, err) => {
                write!(f, "Replay failed on iteration {}: {}", iteration, err)
            }
            DeterminismError::StateMismatch { iteration, expected, actual } => {
                write!(f, "State mismatch on iteration {}: expected {} but got {}", 
                       iteration, expected, actual)
            }
            DeterminismError::EventCountMismatch { iteration, expected, actual } => {
                write!(f, "Event count mismatch on iteration {}: expected {} but got {}", 
                       iteration, expected, actual)
            }
        }
    }
}

impl std::fmt::Display for ConsistencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsistencyError::InsufficientIterations => {
                write!(f, "Need at least 2 iterations for consistency testing")
            }
            ConsistencyError::StateMismatch { iteration, expected, actual } => {
                write!(f, "State mismatch on iteration {}: expected {} but got {}", 
                       iteration, expected, actual)
            }
        }
    }
}

impl std::error::Error for ReplayError {}
impl std::error::Error for DeterminismError {}
impl std::error::Error for ConsistencyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{syzygy::Syzygy, dispatch::Dispatch, resource::Resources};

    #[derive(Debug, Clone, Default, PartialEq)]
    struct TestModel {
        counter: i32,
        messages: Vec<String>,
    }

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment(i32),
        AddMessage(String),
        Reset,
    }

    #[derive(Debug, Clone)]
    enum TestCommand {
        Log(String),
    }

    fn create_test_syzygy() -> (Syzygy<TestModel, TestEvent, TestCommand, Resources>, SyzygyHandle<TestEvent>) {
        Syzygy::builder()
            .model(TestModel::default())
            .event_handler(|event, model| {
                match event {
                    TestEvent::Increment(n) => {
                        model.counter += n;
                        Dispatch::none()
                    }
                    TestEvent::AddMessage(msg) => {
                        model.messages.push(msg);
                        Dispatch::none()
                    }
                    TestEvent::Reset => {
                        model.counter = 0;
                        model.messages.clear();
                        Dispatch::none()
                    }
                }
            })
            .build()
    }

    #[test]
    fn test_record_scenario() {
        let (mut syzygy, handle) = create_test_syzygy();

        let scenario = TestUtils::record_scenario(&mut syzygy, &handle, |h| {
            h.dispatch(TestEvent::Increment(5)).unwrap();
            h.dispatch(TestEvent::AddMessage("hello".to_string())).unwrap();
            h.dispatch(TestEvent::Increment(3)).unwrap();
        });

        assert_eq!(scenario.final_model.counter, 8);
        assert_eq!(scenario.final_model.messages, vec!["hello"]);
        assert_eq!(scenario.recorded_events.len(), 3);
    }

    #[test]
    fn test_deterministic_behavior() {
        use crate::recorder::TimestampedEvent;

        let events = vec![
            TimestampedEvent {
                event: TestEvent::Increment(5),
                timestamp_ms: 0,
            },
            TimestampedEvent {
                event: TestEvent::AddMessage("test".to_string()),
                timestamp_ms: 10,
            },
            TimestampedEvent {
                event: TestEvent::Increment(3),
                timestamp_ms: 20,
            },
        ];

        let result = TestUtils::assert_deterministic(create_test_syzygy, events, 5);
        assert!(result.is_ok(), "System should be deterministic: {:?}", result);
    }

    #[test]
    fn test_consistency() {
        let test_fn = |handle: &SyzygyHandle<TestEvent>| {
            handle.dispatch(TestEvent::Increment(10)).unwrap();
            handle.dispatch(TestEvent::AddMessage("consistent".to_string())).unwrap();
            handle.dispatch(TestEvent::Increment(-3)).unwrap();
        };

        let result = TestUtils::verify_consistency(create_test_syzygy, test_fn, 3);
        assert!(result.is_ok(), "System should be consistent: {:?}", result);
    }

    #[test]
    fn test_config_variations() {
        let step_config = TestUtils::step_config();
        assert!(step_config.step_mode);

        let fast_config = TestUtils::fast_config();
        assert_eq!(fast_config.timeout, Duration::from_millis(100));
        assert!(!fast_config.record_events);
    }
}