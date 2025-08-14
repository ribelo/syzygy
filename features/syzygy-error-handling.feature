@syzygy-error-handling
Feature: Syzygy Error Handling
  As a developer
  I want robust error handling with graceful recovery
  So that I can build resilient applications

  @SYZ-014
  Scenario: Panic Recovery
    Given a Syzygy system with panic recovery
    When an event handler panics during execution
    Then the system should not crash
    And the panic should be caught and logged
    And the system should continue processing other events

  @SYZ-015
  Scenario: Event Error Handling
    Given a Syzygy system with error-as-event pattern
    When an error condition occurs (like validation failure)
    Then the handler should return an error event in the enum
    And the error event should be processed like any other event
    And the processing pipeline should continue normally