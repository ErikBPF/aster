# Bound by crates/catalogs/tests/trino_catalog.rs (HTTP fixture; not live Trino evidence).
Feature: Native Trino catalog metadata
  Scenario: Browse native aliases through complete paginated metadata
    Given separate catalog registrations select native tpch and tpcds aliases
    When namespaces, table descriptors and column schemas are requested
    Then exact aliases and structured namespace identities are retained
    And columns retain their native types and nullability

  Scenario: Treat hostile identifiers as data
    When catalog, namespace or table names contain SQL punctuation
    Then catalog identifiers and metadata filter literals are quoted independently

  Scenario: Refuse untrusted or incomplete observations
    When metadata is malformed, duplicated, incomplete or reports a query error
    Then the complete observation fails without exposing upstream error text
    And off-origin, userinfo, fragment and repeated continuations are refused

  Scenario: Bound complete metadata operations
    When pagination exceeds 64 requests, 4 MiB or 10000 rows
    Then the complete observation fails instead of returning partial metadata
    And all pages share one 10-second deadline
