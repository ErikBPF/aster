# Behavior contract: server API surface.
# Status: bound to `crates/server/tests/contracts.rs` (in-process router).
@contract
Feature: Server API surface
  The server reports readiness and the registered plugin inventory so callers
  can discover engines and catalogs before running anything.

  Scenario: Health endpoint reports readiness
    When the health endpoint is requested
    Then the response status is 200
    And the body is "ok"

  Scenario: Engines are listed with kind, endpoint and health
    When the engine inventory is requested
    Then each engine reports id, kind, endpoint and health

  Scenario: Catalogs are listed with kind and health
    When the catalog inventory is requested
    Then each catalog reports id, kind and health

  Scenario: An unidentified caller cannot read the inventory
    When an unidentified caller reads the engine inventory
    Then the response status is 403

  Scenario: Unknown engine is not found
    When a query is sent to an unknown engine
    Then the response status is 404
