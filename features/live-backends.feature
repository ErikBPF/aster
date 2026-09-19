@contract
Feature: Live compute backends

  Base functionality: a notebook cell runs on a real Trino (through the Trino
  Gateway, so the routing group selects the backend) and on a real Spark Connect
  cluster, both provisioned from platform-gitops. These scenarios are tooling
  assertions against a running cluster, not tested domain behaviour, so they are
  executed by tests/live-backends.sh rather than by a Gherkin runner and are not
  part of `just ci` (they need a reachable cluster and port-forwards). Run them
  with `tests/live-backends.sh`.

  Background:
    Given the platform cluster is reachable through the platform context
    And port-forwards answer for the Trino Gateway and Spark Connect
    And an aster server is running with engines "trino-gw" and "spark-live"

  Scenario: A cell runs on Trino through the gateway
    When subject "alice" runs "select count(*) as n from tpch.tiny.orders" on engine "trino-gw"
    Then the response carries a column "n"
    And the response carries one row

  Scenario: A cell runs on the Spark cluster
    When subject "alice" runs "select explode(sequence(1, 3)) as n" on engine "spark-live"
    Then the response carries a column "n"
    And the response carries three rows

  Scenario: The routing group reaches the gateway
    When subject "alice" runs "select 1" on engine "trino-gw"
    Then the response carries one row
    And the gateway lists exactly one active backend in routing group "lab"

  Scenario: An absent routing group falls back instead of refusing
    When a client posts "select 1" to the gateway with routing group "absent"
    Then the gateway answers from the default routing group

  Scenario: A subject without a grant is refused and the refusal is audited
    When subject "bob" runs "select 1" on engine "trino-gw"
    Then the response is a refusal naming the missing grant
    And the audit trail lists a failed attempt for subject "bob"

  Scenario: Both live runs are audited
    When subject "alice" runs queries on both engines
    Then the audit trail lists a successful attempt on engine "trino-gw"
    And the audit trail lists a successful attempt on engine "spark-live"
