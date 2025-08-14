@syzygy-task-execution
Feature: Syzygy Task Execution
  As a developer
  I want reliable async task execution with effect isolation
  So that I can build responsive applications

  @SYZ-012
  Scenario: Async Task Execution
    Given a Syzygy system with async task support
    When I dispatch an event that generates an async task
    And I process events synchronously
    Then tasks should execute asynchronously without blocking the event loop
    When async tasks complete and dispatch back
    Then their effects should be incorporated into the system

  @SYZ-013
  Scenario: Effect Isolation
    Given a Syzygy system with async task support
    When I dispatch an event that triggers side effects
    Then event processing should remain unaffected by task execution
    And tasks should communicate back through events