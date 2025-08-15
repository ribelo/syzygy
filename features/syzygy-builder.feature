@syzygy-builder
Feature: Syzygy Builder Pattern
  As a developer
  I want to use a fluent builder API to create Syzygy systems
  So that I can configure state management with compile-time safety

  @SYZ-001
  Scenario: Builder Pattern API
    Given I want to create a new Syzygy system
    When I use Syzygy::builder()
    Then I should get a SyzygyBuilder in initial stage
    And I should be able to chain configuration methods

  @SYZ-002
  Scenario: Model Configuration
    Given I have a SyzygyBuilder
    When I call .model(state) to set the application state
    Then the builder should store the model
    And require an event handler before building

  @SYZ-003
  Scenario: Event Handler Registration
    Given I have a SyzygyBuilder with a model
    When I call .event_handler(fn(Event, &mut Model) -> Dispatch<Event, Command>)
    Then the builder should store the handler function
    And I should be able to build the system

  @SYZ-009
  Scenario: Resource Configuration
    Given I have a SyzygyBuilder
    When I call .resource(resource) to add shared resources
    Then the resources should be available during command execution
    And resources should be immutable during event processing

  @SYZ-001
  Scenario: System Building
    Given I have configured model, handler, and optional resources
    When I call .build()
    Then I should get a Syzygy instance and SyzygyHandle
    And the system should be ready for event processing