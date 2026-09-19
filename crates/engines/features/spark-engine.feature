@contract @unautomated
Feature: Spark as a compute backend

  Draft: the Spark engine speaks the Spark Connect protocol through the
  apache-spark-connect crate, so a cell runs SQL on a persistent Spark cluster
  the same way it runs on Trino. The scenarios below are exercised today by the
  unit tests in crates/engines/src/lib.rs (`spark_endpoints_become_connect_
  connection_strings`, `spark_values_keep_their_type_on_the_wire`, `a_spark_
  catalog_request_is_refused_before_connecting`, the engine-pool tests) and by
  features/live-backends.feature, which is bound by tests/live-backends.sh
  against a real cluster. Binding them to a Gherkin runner needs a Spark Connect
  fixture, so they stay a draft.

  Background:
    Given a Spark Connect engine "spark-live" is registered

  Scenario: A SQL statement returns columns and rows
    When a cell runs "select 1 as one, 'a' as letter" on engine "spark-live"
    Then the result has columns "one" and "letter"
    And the result has one row

  Scenario: Spark types become readable column types
    When a cell runs "select 1 as one, 1.5 as half, 'a' as letter, true as flag" on engine "spark-live"
    Then column "one" has type "int"
    And column "half" has type "double"
    And column "letter" has type "string"
    And column "flag" has type "bool"

  Scenario: A row cap truncates the result instead of failing
    Given a cell asks for at most 2 rows
    When a cell runs "select explode(sequence(1, 5)) as n" on engine "spark-live"
    Then the result has 2 rows
    And the result is marked truncated

  Scenario: A Spark value keeps its type on the wire
    When a cell selects a null, a bool, an integer, a double, a string, a decimal and a list
    Then each value is carried as its JSON form
    And a decimal and a timestamp keep their text form rather than losing precision

  Scenario: An http endpoint becomes the client's connect string
    Given the engine endpoint is "http://spark-connect-aster:15002"
    Then the client connects with "sc://spark-connect-aster:15002"
    And an https endpoint asks for ssl

  Scenario: A catalog request is refused before connecting
    Given a cell asks for catalog "hive" on engine "spark-live"
    Then the engine is refused because Spark has no catalog switching yet

  Scenario: An unreachable cluster degrades instead of failing the server
    Given the Spark Connect endpoint is not reachable
    Then the engine health is "unavailable"

  Scenario: A Spark error is returned as an engine error, not a panic
    When a cell runs "select * from missing_table" on engine "spark-live"
    Then the engine fails with an error mentioning the statement
