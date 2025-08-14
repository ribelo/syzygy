@syzygy-state-management
Feature: Syzygy State Management
  As a developer
  I want reliable state and resource management
  So that I can build consistent applications

  @SYZ-008
  Scenario: Model Access (Mutable and Immutable)
    Given a Syzygy system with models is available
    When I access models through the model() method
    Then I should be able to read and write model data

  @SYZ-009
  Scenario: Resource Access (Immutable Only)
    Given a Syzygy system with resources is available
    When I access resources through the resource() method
    Then I should only be able to read resource data

  @SYZ-010
  Scenario: Immediate State Consistency
    Given a Syzygy system is processing events
    When I process a batch of events
    Then all state changes should be immediately visible

  @SYZ-011
  Scenario: State Access During Event Processing
    Given a Syzygy system is processing events
    When I access state during event processing
    Then state changes should be isolated per event