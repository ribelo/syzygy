//! Dispatch type for pure event handlers
//!
//! This module provides the Dispatch type which allows event handlers
//! to return both new events and commands as pure data, without side effects.
//! Uses stack-allocated arrays for zero-cost abstractions.

use arrayvec::ArrayVec;

/// Maximum capacity for events and commands on the stack
const STACK_CAPACITY: usize = 8;

/// Result of processing an event in the functional core
///
/// Event handlers return this type to indicate:
/// - New events to be queued for processing
/// - Commands to be executed in the imperative shell
///
/// This enables a pure functional core where handlers have no side effects.
/// Uses stack-allocated ArrayVec for zero heap allocations in typical use cases.
#[derive(Debug, Clone)]
pub struct Dispatch<Event, Command> {
    /// New events to be queued for processing (stack-allocated)
    pub events: ArrayVec<Event, STACK_CAPACITY>,

    /// Commands to be executed by the command executor (stack-allocated)
    pub commands: ArrayVec<Command, STACK_CAPACITY>,
}

impl<Event, Command> Dispatch<Event, Command> {
    /// Create a new Dispatch with the given events and commands
    #[must_use]
    pub fn new(events: Vec<Event>, commands: Vec<Command>) -> Self {
        Self {
            events: ArrayVec::from_iter(events),
            commands: ArrayVec::from_iter(commands),
        }
    }

    /// Create an empty Dispatch (alias for empty)
    pub fn none() -> Self {
        Self {
            events: ArrayVec::new(),
            commands: ArrayVec::new(),
        }
    }

    /// Create a Dispatch with a single event
    pub fn event(event: Event) -> Self {
        let mut events = ArrayVec::new();
        events.push(event);
        Self {
            events,
            commands: ArrayVec::new(),
        }
    }

    /// Create a Dispatch with a single command
    pub fn command(command: Command) -> Self {
        let mut commands = ArrayVec::new();
        commands.push(command);
        Self {
            events: ArrayVec::new(),
            commands,
        }
    }

    /// Push a single event
    pub fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Push a single command
    pub fn push_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Check if both events and commands are empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.commands.is_empty()
    }

    /// Get the number of events
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get the number of commands
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }
}

impl<Event, Command> Default for Dispatch<Event, Command> {
    fn default() -> Self {
        Self::none()
    }
}


// Convenience conversion from a vector of commands (common case)
impl<Event, Command> From<Vec<Command>> for Dispatch<Event, Command> {
    fn from(commands: Vec<Command>) -> Self {
        Self::new(vec![], commands)
    }
}

// Convenience conversion from arrays
impl<Event, Command, const N: usize> From<[Event; N]> for Dispatch<Event, Command> {
    fn from(events: [Event; N]) -> Self {
        Self {
            events: ArrayVec::from_iter(events),
            commands: ArrayVec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        A,
        B,
        C,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestCommand {
        X,
        Y,
        Z,
    }

    #[test]
    fn test_empty_result() {
        let result: Dispatch<TestEvent, TestCommand> = Dispatch::none();
        assert!(result.is_empty());
        assert_eq!(result.event_count(), 0);
        assert_eq!(result.command_count(), 0);
    }

    #[test]
    fn test_events_only() {
        let result: Dispatch<TestEvent, TestCommand> =
            Dispatch::new(vec![TestEvent::A, TestEvent::B], vec![]);
        assert_eq!(result.event_count(), 2);
        assert_eq!(result.command_count(), 0);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_commands_only() {
        let result: Dispatch<TestEvent, TestCommand> =
            Dispatch::new(vec![], vec![TestCommand::X, TestCommand::Y]);
        assert_eq!(result.event_count(), 0);
        assert_eq!(result.command_count(), 2);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_new_with_both() {
        let result = Dispatch::new(vec![TestEvent::A], vec![TestCommand::X, TestCommand::Y]);
        assert_eq!(result.event_count(), 1);
        assert_eq!(result.command_count(), 2);
    }

    #[test]
    fn test_push_methods() {
        let mut result = Dispatch::none();
        result.push_event(TestEvent::A);
        result.push_command(TestCommand::X);

        assert_eq!(result.event_count(), 1);
        assert_eq!(result.command_count(), 1);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_from_array() {
        let result: Dispatch<TestEvent, TestCommand> = [TestEvent::A, TestEvent::B].into();
        assert_eq!(result.event_count(), 2);
        assert_eq!(result.command_count(), 0);
    }

    #[test]
    fn test_iteration() {
        let result: Dispatch<TestEvent, TestCommand> =
            Dispatch::new(vec![TestEvent::A, TestEvent::B], vec![]);
        let events: Vec<_> = result.events.iter().collect();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_none_method() {
        let result: Dispatch<TestEvent, TestCommand> = Dispatch::none();
        assert!(result.is_empty());
        assert_eq!(result.event_count(), 0);
        assert_eq!(result.command_count(), 0);
    }

    #[test]
    fn test_single_constructors() {
        let event_dispatch: Dispatch<TestEvent, TestCommand> = Dispatch::event(TestEvent::A);
        assert_eq!(event_dispatch.event_count(), 1);
        assert_eq!(event_dispatch.command_count(), 0);

        let command_dispatch: Dispatch<TestEvent, TestCommand> = Dispatch::command(TestCommand::X);
        assert_eq!(command_dispatch.event_count(), 0);
        assert_eq!(command_dispatch.command_count(), 1);
    }
}
