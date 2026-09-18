# Aster behavior contract: the Cube catalog provider.
#
# Status: contract-only draft. The mapping from Cube's compiled model to
# namespaces, tables and columns is executed today by the unit tests in
# `crates/catalogs/src/lib.rs` (`a_cube_model_becomes_namespaces_tables_and_columns`,
# `a_namespace_cube_does_not_have_is_empty`), and the HTTP path was exercised
# against a stub serving `/cubejs-api/v1/meta`. Binding these scenarios needs an
# Iceberg/Cube fixture, which arrives with the runtime decision (D6).

@contract @unautomated
Feature: Cube's semantic layer browses like any other catalog

  Background:
    Given a cube catalog named "cube-local" pointed at a Cube deployment
    And "alice" is signed in as an editor

  Scenario: The compiled model is read from the meta endpoint
    When "alice" lists the namespaces of "cube-local"
    Then the namespaces are "cubes" and "views"
    And the request went to "/cubejs-api/v1/meta"

  Scenario: A cube is listed as a table
    When "alice" lists the tables of namespace "cubes"
    Then the tables contain "Orders"

  Scenario: Measures and dimensions become columns
    When "alice" opens the schema of "Orders"
    Then the columns are "Orders.count", "Orders.total", "Orders.status" and "Orders.placed_at"
    And the column "Orders.status" has type "string"

  Scenario: A view is browsable too
    When "alice" lists the tables of namespace "views"
    Then the tables contain "Sales"

  Scenario: An unknown cube is a not-found error
    When "alice" opens the schema of "NotACube"
    Then the response is 404

  Scenario: An unreachable Cube degrades without killing the server
    Given the Cube deployment is not reachable
    When "alice" lists the catalogs
    Then "cube-local" reports health "unavailable"
    And the catalog page shows the error without a stack trace

  Scenario: A custom base path is honoured
    Given the catalog is configured with base path "analytics"
    When "alice" lists the namespaces of "cube-local"
    Then the request went to "/analytics/v1/meta"
