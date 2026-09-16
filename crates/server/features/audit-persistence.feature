# Draft contract (@unautomated): the durability half of the audit trail.
#
# Split out of query-authorization.feature because it needs a live metadata
# database rather than the in-process harness: the shared-state assertions in
# that file run against in-memory stores, while this scenario is about a row
# surviving a process restart. Executed today by the compose smoke run
# (query, restart the server container, the row is still listed) and by
# crates/server/src/store.rs against Postgres; binding it to a runner needs a
# harness that can start a disposable Postgres.

@contract @unautomated
Feature: Audit trail durability

  Background:
    Given the server is running with engine "trino-local" registered

  Scenario: Audit trail survives a server restart
    Given the metadata store is Postgres
    And subject "alice" has run a query
    When the server restarts
    Then the audit event for subject "alice" is still listed
