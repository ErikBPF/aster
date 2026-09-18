@contract
# Bound: every scenario below runs from crates/core/tests/features.rs, so a
# failing parse or selection fails `cargo test -p aster-core`.
Feature: Data contracts
  Contracts arrive as real Open Data Contract Standard documents, which are YAML,
  and describe a table's fields to the user and to the model.

  Scenario: A real ODCS v3.2 document is understood
    Given an ODCS v3.2 contract named "orders" with fields "order_id, total" and owner "data-team"
    When the contract is parsed
    Then the contract is named "orders"
    And the owner is "data-team"
    And the contract has 2 fields
    And the field "total" is a "measure"

  Scenario: A flat JSON contract still parses
    Given a flat contract object with name "orders" and a required field "id"
    When the contract is parsed
    Then the contract is named "orders"
    And the contract has 1 field

  Scenario: A document with no name and no id is refused
    Given a contract document without a name
    When the contract is parsed
    Then the parse is refused as invalid

  Scenario: Only the contract a statement names is offered to the model
    Given a contract named "orders" and a contract named "people"
    When the statement "SELECT * FROM orders" is prepared
    Then the selected contracts are "orders"

  Scenario: A statement naming nothing selects no contract
    Given a contract named "orders" and a contract named "people"
    When the statement "SELECT 1" is prepared
    Then no contract is selected
