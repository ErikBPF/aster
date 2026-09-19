@contract
# Bound: every scenario below runs from crates/core/tests/features.rs, so a
# broken emitter fails `cargo test -p aster-core`.
Feature: Semantic models
  A table's metadata is rendered as the file a team commits: a Cube model for the
  semantic layer, or an Open Data Contract Standard document for the
  producer/consumer agreement.

  Scenario: A catalog table becomes a Cube model
    Given a table "sales" "orders" with columns "order_id bigint, customer varchar, placed_at timestamp"
    When the table is rendered as "cube"
    Then the model declares the cube "orders"
    And the model names the table "sales.orders"
    And the model declares the dimension "customer"
    And the model declares the dimension "placed_at"

  Scenario: A contract measure becomes a Cube measure
    Given a table "sales" "orders" with columns "order_id bigint, total decimal"
    And an ODCS v3.2 contract named "orders" with fields "order_id, total" and owner "data-team"
    When the contract is parsed
    And the table is rendered as "cube"
    Then the model declares the dimension "order_id"
    And the model declares the measure "total"

  Scenario: A table becomes an ODCS v3.2 contract
    Given a table "sales" "orders" with columns "order_id bigint, total decimal"
    When the table is rendered as "odcs"
    And the rendered contract is parsed
    Then the contract is named "orders"
    And the field "order_id" is a "column"

  Scenario: The contract's semantics reach the emitted contract
    Given a table "sales" "orders" with columns "order_id bigint, total decimal"
    And an ODCS v3.2 contract named "orders" with fields "order_id, total" and owner "data-team"
    When the contract is parsed
    And the table is rendered as "odcs"
    And the rendered contract is parsed
    Then the contract is named "orders"
    And the owner is "data-team"
    And the field "total" is a "measure"
