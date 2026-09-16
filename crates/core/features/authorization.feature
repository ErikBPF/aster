@contract
Feature: Authorization decides what a principal may do

  Every request resolves a principal once, then asks one policy function. The
  scenarios below are executed by crates/core/tests/features.rs.

  Background:
    Given an empty grant store

  Scenario: A viewer may read but not run queries
    Given a principal "bob" with roles "viewer"
    When the principal asks to run a query
    Then the request is refused as unauthorized

  Scenario: An editor may run queries
    Given a principal "alice" with roles "editor"
    When the principal asks to run a query
    Then the request is allowed

  Scenario: An administrator may administer
    Given a principal "root" with roles "admin"
    When the principal asks to administer
    Then the request is allowed

  Scenario: A grant is scoped to one engine
    Given a principal "alice" with roles "editor"
    And "alice" is granted engine "trino-local"
    When "alice" asks to use engine "trino-local"
    Then the engine request is allowed
    When "alice" asks to use engine "trino-other"
    Then the engine request is refused as unauthorized

  Scenario: An unknown subject has no engine access
    Given a principal "mallory" with roles "editor"
    When "mallory" asks to use engine "trino-local"
    Then the engine request is refused as unauthorized
