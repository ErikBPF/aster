# Aster behavior contract: data contracts (ODCS-shaped).
#
# Status: contract-only. The equivalent Rust tests (`crates/core/src/contract.rs`,
# `crates/server/src/contracts.rs`) plus the server smoke path execute these behaviors today;
# binding them to a Gherkin runner is tracked in the platform proposal.

@contract @unautomated
Feature: Data contracts describe tables to users and to the model

  Background:
    Given the server is running with ASTER_CONTRACTS_DIR pointing at a directory of contracts
    And "alice" is signed in as an editor

  Scenario: A contract file is listed
    Given a contract file "orders.json" with name "orders" and a required "id" field
    When "alice" opens /contracts
    Then the page names the contract "orders"
    And the page lists the field "id"

  Scenario: A real ODCS v3.2 yaml contract is listed
    Given a contract file "orders.odcs.yaml" written in ODCS v3.2 with an "orders" schema of three properties
    When "alice" opens /contracts
    Then the page names the contract "orders"
    And the page lists the field "total" marked as a "measure"

  Scenario: Contracts are visible over the API
    When "alice" requests /api/contracts
    Then the response contains a contract named "orders"
    And the response does not contain any credential

  Scenario: An unauthenticated caller sees no contracts
    When an unidentified caller requests /api/contracts
    Then the response is 403

  Scenario: A malformed contract is skipped without stopping the server
    Given a contract file "broken.json" that is not valid JSON
    When the server starts
    Then the server logs a warning naming "broken.json"
    And the server still serves the remaining contracts

  Scenario: A non-contract file is ignored
    Given a contract directory containing "notes.txt"
    When the server starts
    Then no contract is loaded from it

  Scenario: A contract named in the cell is offered to the model
    Given "alice" has registered an llm endpoint
    And the cell contains "SELECT * FROM orders"
    When "alice" asks for a query suggestion
    Then the request sent to the llm includes the contract summary for "orders"

  Scenario: A contract not named in the cell is not sent
    Given "alice" has registered an llm endpoint
    And the cell contains "SELECT * FROM people"
    When "alice" asks for a query suggestion
    Then the request sent to the llm does not include the contract summary for "orders"
