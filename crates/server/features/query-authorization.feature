# Behavior contract: query authorization and audit.
# Status: bound to `crates/server/tests/contracts.rs`, which drives the router
# in process with in-memory stores and a stub engine. The durability scenario
# needs a live database and lives in features/audit-persistence.feature.
@contract
Feature: Query authorization and audit
  A query runs only when the caller is identified, holds a role allowed to run
  queries, and is granted access to the selected engine. Every attempt is
  audited, whatever its outcome.

  Background:
    Given the server is running with engine "trino-local" registered

  Scenario: Unidentified caller is refused
    When a query is sent without a subject header
    Then the response status is 403

  Scenario: Caller without an engine grant is refused
    Given subject "bob" has no grant for engine "trino-local"
    When subject "bob" sends a query to "trino-local"
    Then the response status is 403
    And the error names the missing grant
    And an audit event is recorded for subject "bob"

  Scenario: Granted caller reaches the engine
    Given subject "alice" is granted engine "trino-local"
    When subject "alice" sends a query to "trino-local"
    Then the request is forwarded to the engine
    And an audit event is recorded for subject "alice"

  Scenario: A statement the engine refuses is surfaced as a query error
    Given subject "alice" is granted engine "trino-local"
    And the engine refuses the statement with "Catalog must be specified when session catalog is not set"
    When subject "alice" sends a query to "trino-local"
    Then the response status is 400
    And the error names the engine's reason

  Scenario: The configured default catalog becomes the engine's session catalog
    Given subject "alice" is granted engine "trino-local"
    And the default catalog is "polaris"
    When subject "alice" sends a query to "trino-local"
    Then the query ran with catalog "polaris"

  Scenario: An engine with no session catalog is not given the default
    Given subject "alice" is granted engine "trino-local"
    And the default catalog is "polaris"
    And the engine has no session catalog
    When subject "alice" sends a query to "trino-local"
    Then the query ran without a catalog

  Scenario: Viewer role may not run queries
    Given subject "carol" has only the "viewer" role
    When subject "carol" sends a query
    Then the response status is 403

  Scenario: Audit trail is admin-only
    When subject "alice" with role "editor" reads the audit trail
    Then the response status is 403
    When subject "root" with role "admin" reads the audit trail
    Then the response status is 200
