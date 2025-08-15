@syzygy-core-system
Feature: Syzygy Core System
  As a developer
  I want a complete event-driven state management system
  So that I can build reliable, testable applications

  @SYZ-002 @SYZ-003
  Scenario: Model-Based State Management
    Given I want to create a Syzygy system with state
    When I use the builder to set a model and event handler
    Then the system should provide controlled access to the model
    And event handlers should receive mutable model references

  @SYZ-004 @SYZ-028
  Scenario: Channel-Based Event Interface
    Given a Syzygy system is configured and built
    When I use the SyzygyHandle to dispatch events
    Then events should be queued through channels
    And dispatch calls should be non-blocking

  @SYZ-027
  Scenario: Single-Threaded Event Processing
    Given multiple events are queued
    When I call process_events()
    Then all events should be processed sequentially
    And state changes should be deterministic

  @SYZ-014
  Scenario: Panic Recovery
    Given an event handler that may panic
    When the handler panics during event processing
    Then the system should continue running
    And subsequent events should still be processed
