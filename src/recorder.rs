//! Event recording for deterministic testing
//!
//! This module provides event recording capabilities that enable deterministic
//! testing by capturing exact event sequences for later replay.

use std::time::Instant;
use serde::{Serialize, Deserialize};

/// Records events with timestamps for deterministic replay
#[derive(Debug, Clone)]
pub struct EventRecorder<E> {
    events: Vec<TimestampedEvent<E>>,
    start_time: Option<Instant>,
    recording: bool,
}

/// An event with its timestamp relative to recording start
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedEvent<E> {
    pub event: E,
    pub timestamp_ms: u64,
}

impl<E> Default for EventRecorder<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> EventRecorder<E> {
    /// Create a new event recorder
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            start_time: None,
            recording: false,
        }
    }

    /// Start recording events
    pub fn start_recording(&mut self) {
        self.events.clear();
        self.start_time = Some(Instant::now());
        self.recording = true;
    }

    /// Stop recording events
    pub fn stop_recording(&mut self) {
        self.recording = false;
    }

    /// Record an event if recording is active
    pub fn record_event(&mut self, event: E) {
        if self.recording {
            if let Some(start_time) = self.start_time {
                let timestamp_ms = start_time.elapsed().as_millis() as u64;
                self.events.push(TimestampedEvent {
                    event,
                    timestamp_ms,
                });
            }
        }
    }

    /// Get the recorded event sequence
    pub fn get_sequence(&self) -> &[TimestampedEvent<E>] {
        &self.events
    }

    /// Get just the events without timestamps
    pub fn get_events(&self) -> Vec<E> 
    where 
        E: Clone,
    {
        self.events.iter().map(|te| te.event.clone()).collect()
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Get the number of recorded events
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Clear all recorded events
    pub fn clear(&mut self) {
        self.events.clear();
        self.start_time = None;
        self.recording = false;
    }
}

impl<E> EventRecorder<E>
where
    E: Serialize,
{
    /// Save the recorded sequence to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.events)
    }

    /// Save sequence to a file path
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

impl<E> EventRecorder<E>
where
    E: for<'de> Deserialize<'de>,
{
    /// Load sequence from JSON string
    pub fn from_json(json: &str) -> Result<Vec<TimestampedEvent<E>>, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Load sequence from a file path
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<TimestampedEvent<E>>, Box<dyn std::error::Error>> {
        let json = std::fs::read_to_string(path)?;
        let events = Self::from_json(&json)?;
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TestEvent {
        Increment(i32),
        Reset,
    }

    #[test]
    fn test_event_recorder_basic() {
        let mut recorder = EventRecorder::new();
        
        assert!(!recorder.is_recording());
        assert_eq!(recorder.event_count(), 0);

        recorder.start_recording();
        assert!(recorder.is_recording());

        recorder.record_event(TestEvent::Increment(1));
        recorder.record_event(TestEvent::Increment(2));
        recorder.record_event(TestEvent::Reset);

        assert_eq!(recorder.event_count(), 3);

        recorder.stop_recording();
        assert!(!recorder.is_recording());

        let events = recorder.get_events();
        assert_eq!(events, vec![
            TestEvent::Increment(1),
            TestEvent::Increment(2),
            TestEvent::Reset,
        ]);
    }

    #[test]
    fn test_event_recorder_timestamps() {
        use std::time::Duration;
        
        let mut recorder = EventRecorder::new();
        
        recorder.start_recording();
        recorder.record_event(TestEvent::Increment(1));
        
        // Small delay to ensure different timestamps
        std::thread::sleep(Duration::from_millis(1));
        
        recorder.record_event(TestEvent::Increment(2));

        let sequence = recorder.get_sequence();
        assert_eq!(sequence.len(), 2);
        assert!(sequence[1].timestamp_ms >= sequence[0].timestamp_ms);
    }

    #[test]
    fn test_event_recorder_no_recording() {
        let mut recorder = EventRecorder::new();
        
        // Events should not be recorded when not recording
        recorder.record_event(TestEvent::Increment(1));
        assert_eq!(recorder.event_count(), 0);
    }

    #[test]
    fn test_event_recorder_clear() {
        let mut recorder = EventRecorder::new();
        
        recorder.start_recording();
        recorder.record_event(TestEvent::Increment(1));
        recorder.record_event(TestEvent::Reset);
        
        assert_eq!(recorder.event_count(), 2);
        
        recorder.clear();
        assert_eq!(recorder.event_count(), 0);
        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_event_recorder_json_serialization() {
        let mut recorder = EventRecorder::new();
        
        recorder.start_recording();
        recorder.record_event(TestEvent::Increment(42));
        recorder.record_event(TestEvent::Reset);
        
        let json = recorder.to_json().unwrap();
        assert!(json.contains("Increment"));
        assert!(json.contains("42"));
        assert!(json.contains("Reset"));
        assert!(json.contains("timestamp_ms"));

        let loaded_events = EventRecorder::<TestEvent>::from_json(&json).unwrap();
        assert_eq!(loaded_events.len(), 2);
        assert_eq!(loaded_events[0].event, TestEvent::Increment(42));
        assert_eq!(loaded_events[1].event, TestEvent::Reset);
    }
}