//! Event replay for deterministic testing
//!
//! This module provides event replay capabilities that enable deterministic
//! testing by replaying exact event sequences with controlled timing.

use crate::recorder::TimestampedEvent;
use std::time::{Duration, Instant};
use serde::Deserialize;

/// Replays events from a recorded sequence with timing control
#[derive(Debug, Clone)]
pub struct EventReplayer<E> {
    events: Vec<TimestampedEvent<E>>,
    position: usize,
    start_time: Option<Instant>,
    timing_mode: TimingMode,
}

/// Controls how event timing is handled during replay
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimingMode {
    /// Replay events immediately without delays
    Immediate,
    /// Replay events with original timing preserved
    RealTime,
    /// Replay events with accelerated timing (multiplier)
    Accelerated(f64),
}

/// Result of attempting to get the next event during replay
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayResult<E> {
    /// Event is ready to be dispatched
    Event(E),
    /// Need to wait before next event is ready
    Wait(Duration),
    /// Replay sequence is complete
    Complete,
}

impl<E> EventReplayer<E> {
    /// Create a new event replayer from a sequence
    pub fn new(events: Vec<TimestampedEvent<E>>) -> Self {
        Self {
            events,
            position: 0,
            start_time: None,
            timing_mode: TimingMode::Immediate,
        }
    }

    /// Create an empty replayer
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    /// Set the timing mode for replay
    pub fn with_timing_mode(mut self, timing_mode: TimingMode) -> Self {
        self.timing_mode = timing_mode;
        self
    }

    /// Start the replay sequence
    pub fn start(&mut self) {
        self.position = 0;
        self.start_time = Some(Instant::now());
    }

    /// Reset the replay to the beginning without starting
    pub fn reset(&mut self) {
        self.position = 0;
        self.start_time = None;
    }

    /// Get the next event, respecting timing mode
    pub fn next_event(&mut self) -> ReplayResult<E> 
    where 
        E: Clone,
    {
        if self.position >= self.events.len() {
            return ReplayResult::Complete;
        }

        let event = &self.events[self.position];

        match self.timing_mode {
            TimingMode::Immediate => {
                self.position += 1;
                ReplayResult::Event(event.event.clone())
            }
            TimingMode::RealTime => {
                self.next_event_with_timing(1.0)
            }
            TimingMode::Accelerated(multiplier) => {
                self.next_event_with_timing(multiplier)
            }
        }
    }

    /// Get next event with timing calculations
    fn next_event_with_timing(&mut self, time_multiplier: f64) -> ReplayResult<E>
    where 
        E: Clone,
    {
        if let Some(start_time) = self.start_time {
            let event = &self.events[self.position];
            let target_time_ms = (event.timestamp_ms as f64 / time_multiplier) as u64;
            let elapsed_ms = start_time.elapsed().as_millis() as u64;

            if elapsed_ms >= target_time_ms {
                self.position += 1;
                ReplayResult::Event(event.event.clone())
            } else {
                let wait_ms = target_time_ms - elapsed_ms;
                ReplayResult::Wait(Duration::from_millis(wait_ms))
            }
        } else {
            // Not started yet, return first event immediately
            self.position += 1;
            ReplayResult::Event(self.events[0].event.clone())
        }
    }

    /// Peek at the next event without advancing position
    pub fn peek_next(&self) -> Option<&TimestampedEvent<E>> {
        self.events.get(self.position)
    }

    /// Get all remaining events immediately (ignores timing)
    pub fn remaining_events(&self) -> Vec<E> 
    where 
        E: Clone,
    {
        self.events[self.position..].iter()
            .map(|te| te.event.clone())
            .collect()
    }

    /// Check if replay is complete
    pub fn is_complete(&self) -> bool {
        self.position >= self.events.len()
    }

    /// Get current position in the sequence
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get total number of events in sequence
    pub fn total_events(&self) -> usize {
        self.events.len()
    }

    /// Get progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.events.is_empty() {
            1.0
        } else {
            self.position as f64 / self.events.len() as f64
        }
    }

    /// Get the timing mode
    pub fn timing_mode(&self) -> TimingMode {
        self.timing_mode
    }

    /// Set a new timing mode
    pub fn set_timing_mode(&mut self, timing_mode: TimingMode) {
        self.timing_mode = timing_mode;
    }
}

impl<E> EventReplayer<E>
where
    E: for<'de> Deserialize<'de>,
{
    /// Create replayer from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let events = serde_json::from_str(json)?;
        Ok(Self::new(events))
    }

    /// Create replayer from file path
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let json = std::fs::read_to_string(path)?;
        let replayer = Self::from_json(&json)?;
        Ok(replayer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::TimestampedEvent;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestEvent {
        Increment(i32),
        Reset,
    }

    fn create_test_events() -> Vec<TimestampedEvent<TestEvent>> {
        vec![
            TimestampedEvent {
                event: TestEvent::Increment(1),
                timestamp_ms: 0,
            },
            TimestampedEvent {
                event: TestEvent::Increment(2),
                timestamp_ms: 100,
            },
            TimestampedEvent {
                event: TestEvent::Reset,
                timestamp_ms: 200,
            },
        ]
    }

    #[test]
    fn test_event_replayer_immediate() {
        let events = create_test_events();
        let mut replayer = EventReplayer::new(events)
            .with_timing_mode(TimingMode::Immediate);

        assert_eq!(replayer.total_events(), 3);
        assert_eq!(replayer.position(), 0);
        assert!(!replayer.is_complete());

        // Get all events immediately
        assert_eq!(replayer.next_event(), ReplayResult::Event(TestEvent::Increment(1)));
        assert_eq!(replayer.next_event(), ReplayResult::Event(TestEvent::Increment(2)));
        assert_eq!(replayer.next_event(), ReplayResult::Event(TestEvent::Reset));
        assert_eq!(replayer.next_event(), ReplayResult::Complete);

        assert!(replayer.is_complete());
        assert_eq!(replayer.progress(), 1.0);
    }

    #[test]
    fn test_event_replayer_reset() {
        let events = create_test_events();
        let mut replayer = EventReplayer::new(events);

        // Advance position
        replayer.next_event();
        replayer.next_event();
        assert_eq!(replayer.position(), 2);

        // Reset
        replayer.reset();
        assert_eq!(replayer.position(), 0);
        assert!(!replayer.is_complete());
    }

    #[test]
    fn test_event_replayer_peek() {
        let events = create_test_events();
        let mut replayer = EventReplayer::new(events);

        // Peek at first event
        let peeked = replayer.peek_next().unwrap();
        assert_eq!(peeked.event, TestEvent::Increment(1));
        assert_eq!(replayer.position(), 0); // Position shouldn't change

        // Advance and peek at second event
        replayer.next_event();
        let peeked = replayer.peek_next().unwrap();
        assert_eq!(peeked.event, TestEvent::Increment(2));
        assert_eq!(replayer.position(), 1);
    }

    #[test]
    fn test_event_replayer_remaining_events() {
        let events = create_test_events();
        let mut replayer = EventReplayer::new(events);

        // Get first event
        replayer.next_event();

        // Get remaining events
        let remaining = replayer.remaining_events();
        assert_eq!(remaining, vec![TestEvent::Increment(2), TestEvent::Reset]);
    }

    #[test]
    fn test_event_replayer_progress() {
        let events = create_test_events();
        let mut replayer = EventReplayer::new(events);

        assert_eq!(replayer.progress(), 0.0);

        replayer.next_event();
        assert!((replayer.progress() - 0.333).abs() < 0.01);

        replayer.next_event();
        assert!((replayer.progress() - 0.666).abs() < 0.01);

        replayer.next_event();
        assert_eq!(replayer.progress(), 1.0);
    }

    #[test]
    fn test_event_replayer_empty() {
        let mut replayer = EventReplayer::<TestEvent>::empty();

        assert_eq!(replayer.total_events(), 0);
        assert!(replayer.is_complete());
        assert_eq!(replayer.progress(), 1.0);
        assert_eq!(replayer.next_event(), ReplayResult::Complete);
    }

    #[test]
    fn test_timing_modes() {
        let events = create_test_events();
        let mut replayer = EventReplayer::new(events);

        // Test immediate mode
        replayer.set_timing_mode(TimingMode::Immediate);
        assert_eq!(replayer.timing_mode(), TimingMode::Immediate);

        // Test real-time mode
        replayer.set_timing_mode(TimingMode::RealTime);
        assert_eq!(replayer.timing_mode(), TimingMode::RealTime);

        // Test accelerated mode
        replayer.set_timing_mode(TimingMode::Accelerated(2.0));
        assert_eq!(replayer.timing_mode(), TimingMode::Accelerated(2.0));
    }

    #[test]
    fn test_real_time_replay() {
        let events = vec![
            TimestampedEvent {
                event: TestEvent::Increment(1),
                timestamp_ms: 0,
            },
            TimestampedEvent {
                event: TestEvent::Increment(2),
                timestamp_ms: 50, // 50ms later
            },
        ];

        let mut replayer = EventReplayer::new(events)
            .with_timing_mode(TimingMode::RealTime);

        replayer.start();

        // First event should be available immediately
        assert_eq!(replayer.next_event(), ReplayResult::Event(TestEvent::Increment(1)));

        // Second event should require waiting (unless 50ms have passed)
        match replayer.next_event() {
            ReplayResult::Wait(duration) => {
                assert!(duration.as_millis() <= 50);
            }
            ReplayResult::Event(TestEvent::Increment(2)) => {
                // This is also valid if enough time has passed
            }
            _ => panic!("Unexpected result"),
        }
    }
}