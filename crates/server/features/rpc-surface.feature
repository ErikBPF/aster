# Behavior contract: the multiprotocol RPC surface.
# Status: bound to `crates/server/tests/contracts.rs`. The Connect leg is called
# as JSON through the router; the gRPC leg sends a framed `application/grpc`
# request on the same route and asserts the response is a gRPC-framed protobuf
# naming the same engine (a real client-level gRPC check stays in the
# `grpcurl` smoke documented in the README).
@contract
Feature: Multiprotocol RPC surface

  Background:
    Given the server runs with engine "trino-local" and catalog "polaris-local"
    And subject "alice" holds the editor role with a grant on "trino-local"

  Scenario: The same proto answers gRPC and Connect JSON
    When "alice" calls ListEngines over the Connect protocol as JSON
    Then the response lists "trino-local" with its kind, endpoint and health
    And the same call over gRPC returns the same engines

  Scenario: Working state round-trips over RPC
    Given "alice" currently has notebook "sales" open at cell "c2"
    When "alice" calls PutState with that working state
    And "alice" calls GetState
    Then the returned state names notebook "sales" and cell "c2"

  Scenario: Notebooks are read and saved over RPC
    When "alice" saves a notebook "sales" with one cell "SELECT 1"
    Then the response carries the commit revision
    And GetNotebook returns the same cell

  Scenario: A query runs on the engine the caller picked
    When "alice" calls RunQuery on "trino-local" with "SELECT 1"
    Then the call reaches the engine
    And the audit trail records the query for "alice"

  Scenario: An unauthenticated caller is refused
    When an unidentified caller calls ListNotebooks
    Then the call fails as unauthenticated
    And no notebook content is returned

  Scenario: A caller without an engine grant is refused
    Given "bob" holds the editor role without a grant on "trino-local"
    When "bob" calls RunQuery on "trino-local"
    Then the call fails as permission denied naming the missing grant

  Scenario: A query without a selected engine and no default is refused
    Given the server has no default engine
    When "alice" calls RunQuery without selecting an engine
    Then the call fails as invalid input

  Scenario: A table renders as the Cube model a team commits
    When "alice" renders the "default" table "orders" as "cube"
    Then the response names the cube "orders" and its path

  Scenario: The same table renders as an ODCS contract
    When "alice" renders the "default" table "orders" as "odcs"
    Then the response carries an odcs contract with fields

  Scenario: An unknown render target is refused
    When "alice" renders the "default" table "orders" as "lookml"
    Then the call fails as invalid input
