@contract @unautomated
Feature: Engine pool and routing groups

  Engines are configured as a pool. When an instance sits behind a gateway the
  routing group selects the backend pool, so a cell can choose where to run.

  Scenario: A pool is configured as a compact list
    Given ASTER_ENGINES is "adhoc;trino;http://gw:8080;adhoc,etl;trino;http://gw:8080;etl"
    Then two engines are registered
    And engine "adhoc" has routing group "adhoc"
    And engine "etl" has routing group "etl"

  Scenario: A routing group is passed to the gateway
    Given engine "adhoc" has routing group "adhoc"
    When a cell runs on engine "adhoc"
    Then the engine request carries header "X-Trino-Routing-Group: adhoc"

  Scenario: An instance without a routing group sends no routing header
    Given engine "standalone" has no routing group
    When a cell runs on engine "standalone"
    Then the engine request carries no "X-Trino-Routing-Group" header

  Scenario: Catalog and schema travel only when the cell asks for them
    Given a cell asks for catalog "tpch" and schema "tiny" on engine "adhoc"
    Then the engine request carries header "X-Trino-Catalog: tpch"
    And the engine request carries header "X-Trino-Schema: tiny"

  Scenario: A follow-up page is sent to the configured endpoint
    Given engine "adhoc" points at "http://gateway:8080"
    When the coordinator advertises a next page on its own address
    Then the follow-up request goes to "http://gateway:8080"

  Scenario: An unknown engine kind is refused at startup
    Given ASTER_ENGINES is "x;duckdb;http://localhost:1"
    Then startup fails naming the unknown engine kind

  Scenario: The engine list exposes the routing group
    When the client requests "/api/engines"
    Then each engine entry carries "id", "kind", "endpoint", "routing_group" and "health"
