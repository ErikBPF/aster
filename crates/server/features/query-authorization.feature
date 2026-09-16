# Behavior contract: query authorization and audit.
# Status: contract only (@unautomated). The equivalent Rust tests in
# crates/core (auth, grants, audit) and the server smoke path currently
# execute these behaviors; binding to a Gherkin runner is tracked in the
# aster proposal (IP S2+).
@contract @unautomated
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

  Scenario: Granted caller reaches the engine
    Given subject "alice" is granted engine "trino-local"
    When subject "alice" sends a query to "trino-local"
    Then the request is forwarded to the engine
    And an audit event is recorded for subject "alice"

  Scenario: Viewer role may not run queries
    Given subject "carol" has only the "viewer" role
    When subject "carol" sends a query
    Then the response status is 403

  Scenario: Audit trail is admin-only
    When subject "alice" with role "editor" reads the audit trail
    Then the response status is 403
    When subject "root" with role "admin" reads the audit trail
    Then the response status is 200

  Scenario: Audit trail survives a server restart
    Given the metadata store is Postgres
    And subject "alice" has run a query
    When the server restarts
    Then the audit event for subject "alice" is still listed
