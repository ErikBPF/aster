# Draft behavior contract from the 2026-09-22 user clarification.
# Unbound: planned server router bindings and local catalog/compute E2E.
@contract @unautomated
Feature: Catalog-bound compute and data access
  A selected catalog determines the data context and connected compute choices.
  Changing catalog providers must not retain an incompatible execution target.

  Scenario: List only compute connected to the selected catalog
    Given catalog "lake-a" is connected to engines "trino-a" and "spark-a"
    And catalog "lake-b" is connected only to engine "spark-b"
    And the caller is granted all three engines
    When the caller selects catalog "lake-a"
    Then the available compute choices are "trino-a" and "spark-a"
    And engine "spark-b" is not offered

  Scenario: Refuse a disconnected engine even when the request bypasses the UI
    Given catalog "lake-a" is not connected to engine "spark-b"
    And the caller has an engine grant for "spark-b"
    When the caller submits a query for catalog "lake-a" on engine "spark-b"
    Then the request is refused before engine execution
    And the refused attempt is audited

  Scenario: Changing catalog invalidates incompatible compute and data context
    Given the caller selected a table and engine belonging to catalog "lake-a"
    When the caller changes to catalog "lake-b" with different connected engines
    Then the old engine and table are not used for the next query
    And the caller must select a valid execution context

  Scenario: Engine admission does not override catalog data denial
    Given the caller can use an engine connected to the selected catalog
    And the authoritative data policy denies the requested table
    When the caller queries that table through Aster
    Then no denied table data is returned

  Scenario: Fully qualified SQL cannot bypass the selected catalog access boundary
    Given the caller successfully read an allowed fixture through a connected engine
    And the caller is denied data in catalog "foreign_catalog"
    When the caller submits fully qualified SQL for "foreign_catalog.private.orders" on that engine
    Then the authoritative backend denies the data access
    And no foreign table rows are returned

  Scenario: A direct Delta storage path cannot bypass data policy
    Given the caller successfully read an allowed Delta fixture through a connected engine
    And storage policy denies the caller access to "s3://denied-prefix/orders"
    When the caller queries "delta.`s3://denied-prefix/orders`" on that engine
    Then the authoritative backend denies the data access
    And no denied Delta rows are returned

  Scenario: Prove shared Iceberg data after the Delta generic-table evaluation
    Given the Polaris Delta catalog, connected-engine data and denial checks completed successfully
    And Trino and Spark are connected to the same Polaris Iceberg catalog
    And a fixture table contains rows "1,alpha" and "2,beta"
    When the caller browses the table and reads it with each connected engine
    Then both engines return the same ordered fixture rows
    And the browsed table resolves to the same catalog table identity

  Scenario: The backend applies different data permissions to two admitted users
    Given Alice and Bob are granted the same connected engine
    And catalog or engine policy permits Alice to read "sales.orders" but denies Bob
    When both users query "sales.orders" through Aster
    Then Alice receives the allowed fixture rows
    And the backend denies Bob's query under Bob's effective identity
    And no session or cached credential from Alice grants Bob access

  Scenario: Missing delegated identity fails closed
    Given the selected engine cannot establish the caller's authenticated data identity
    When the caller submits a query
    Then execution is refused
    And no shared privileged credential is used as a fallback

  Scenario: Protected Spark requires proxy identity and isolated backend access
    Given Alice and Bob have the same Aster engine grant for protected Spark
    And an authenticating gRPC proxy binds each request to the verified caller
    And Spark's catalog and storage policy permits Alice but denies Bob
    When each caller reads the same connected Delta table through Aster
    Then Alice receives the independently seeded rows
    And Bob receives a backend data denial
    And Bob cannot reuse Alice's Spark session or storage credential
    And Bob's direct object request is denied under Bob's effective storage identity
