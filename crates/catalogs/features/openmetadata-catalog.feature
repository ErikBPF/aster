# Aster behavior contract: OpenMetadata as a metadata source.
#
# Status: contract-only draft. The mapping from OpenMetadata's list responses to
# namespaces, tables and columns is executed today by the unit tests in
# `crates/catalogs/src/lib.rs` (`openmetadata_schemas_and_tables_become_namespaces_and_table_refs`,
# `openmetadata_columns_carry_types_and_nullability`), and the HTTP path was
# exercised against a stub serving `/api/v1/databaseSchemas` and `/api/v1/tables`.
# Binding these scenarios needs a live OpenMetadata fixture (it needs a database
# and a search backend of its own), which arrives with the runtime decision (D6).

@contract @unautomated
Feature: OpenMetadata tables are browsable in the catalog tab

  Background:
    Given an openmetadata catalog named "om-local" pointed at an OpenMetadata deployment
    And "alice" is signed in as an editor

  Scenario: Schemas are listed as namespaces
    When "alice" lists the namespaces of "om-local"
    Then the namespaces contain "trino.platform.sales"
    And the request went to "/api/v1/databaseSchemas"

  Scenario: Tables are listed for a schema
    When "alice" lists the tables of namespace "trino.platform.sales"
    Then the tables contain "orders"

  Scenario: Columns carry their type and nullability
    When "alice" opens the schema of "orders"
    Then the columns contain "order_id" and "total"
    And the column "total" has type "DECIMAL(12,2)"
    And the column "order_id" is not nullable

  Scenario: An unconstrained column is nullable
    When "alice" opens the schema of "orders"
    Then the column "placed_at" is nullable

  Scenario: Only the configured database is offered
    Given the catalog is configured with database "trino.platform"
    When "alice" lists the namespaces of "om-local"
    Then the request carries the database filter

  Scenario: An unknown table is a not-found error
    When "alice" opens the schema of "nope"
    Then the response is 404

  Scenario: An unreachable OpenMetadata degrades without killing the server
    Given the OpenMetadata deployment is not reachable
    When "alice" lists the catalogs
    Then "om-local" reports health "unavailable"
    And the catalog page shows the error without a stack trace

  Scenario: No token is sent when none is configured
    When "alice" lists the namespaces of "om-local"
    Then the request carries no Authorization header
