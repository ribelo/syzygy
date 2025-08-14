@syzygy-event-processing
Feature: Syzygy Event Processing
  As a developer
  I want reliable and pure event processing
  So that I can build predictable applications

  @SYZ-004
  Scenario: Non-Blocking Event Dispatch
    Given a Syzygy system is running
    When I dispatch multiple events through the handle
    Then the dispatch calls should not block
    And events should be queued for processing

  @SYZ-005
  Scenario: Sequential Event Processing
    Given a Syzygy system is running
    When I dispatch events in a specific order
    And I process all events
    Then events should be processed in FIFO order

  @SYZ-006
  Scenario: Pure Event Handlers
    Given a Syzygy system is running
    When I dispatch an event that generates new events and commands
    Then the handler should return Dispatch with new effects as data
    And the handler should be a pure function

  @SYZ-007
  Scenario: Dispatch Builder Pattern
    Given I want to create Dispatch instances
    When I use the Dispatch builder methods like none(), event(), command()
    Then I should be able to create results with zero or many events and commands