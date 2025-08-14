@syzygy-builder
Feature: Syzygy Builder Pattern
  As a developer
  I want to use a fluent builder API to create Syzygy systems
  So that I can configure state management with compile-time safety

  @SYZ-001
  Scenario: Builder Pattern API
    Given I want to create a new Syzygy system
    When I use the builder pattern API
    Then I should be able to configure state and event handlers