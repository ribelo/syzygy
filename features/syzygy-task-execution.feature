@syzygy-command-execution
Feature: Syzygy Command Execution
  As a developer
  I want reliable async command execution with effect isolation
  So that I can build responsive applications

  @SYZ-012
  Scenario: Async Command Execution
    Given a Syzygy system with command handler configured
    When I dispatch an event that generates commands
    And I process events synchronously
    Then commands should be sent to the command executor
    And command execution should not block event processing

  @SYZ-013
  Scenario: Effect Isolation
    Given a Syzygy system with command execution
    When I dispatch an event that triggers side effects through commands
    Then event processing should remain pure and unaffected
    And commands should communicate back through events

  @SYZ-012
  Scenario: Command Context Access
    Given a command handler is executing
    When the command needs to access resources
    Then it should receive a CommandContext with resource access
    And it should be able to dispatch new events back to the system

  @SYZ-022 @SYZ-023
  Scenario: Side Effects as Data
    Given an event handler processes business logic
    When it needs to trigger I/O or side effects
    Then it should return commands as data structures
    And not perform the side effects directly