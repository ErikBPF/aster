@contract
Feature: Notebook sessions and the notebook-wide engine

  A notebook and each cell keep server-owned AI sessions. The latest session is
  listed first; older sessions remain selectable. The notebook document owns one
  engine, and Markdown cells never execute.

  Background:
    Given the server is running with engine "trino-local" registered
    And subject "alice" has saved notebook "sales" with one cell "SELECT 1"

  Scenario: Starting a notebook session makes it the latest
    Given Alice has an existing notebook session for "sales"
    When Alice starts a notebook session for "sales"
    Then the notebook session list for "sales" starts with the new session
    And the previous notebook session remains selectable

  Scenario: Cell sessions do not mix with notebook sessions
    Given Alice has an existing notebook session for "sales"
    When Alice starts a cell session for "sales" cell "c1"
    Then the cell session list for "sales" cell "c1" holds only the new cell session
    And the notebook session list for "sales" does not hold the cell session

  Scenario: The notebook engine is authoritative
    Given subject "alice" has saved notebook "engine-nb" with engine "trino-local" and cell "c1" holding "SELECT 1"
    And engine "other-engine" is also registered and granted
    When Alice runs cell "c1" of notebook "engine-nb" with engine "other-engine"
    Then the request is refused because the notebook selects its engine
    And the engine was not called
    When Alice runs cell "c1" of notebook "engine-nb" without selecting an engine
    Then the call reaches the engine

  Scenario: A Markdown cell never executes
    Given subject "alice" has saved notebook "notes" with engine "trino-local" and cell "m1" holding "%md"
    When Alice runs cell "m1" of notebook "notes" without selecting an engine
    Then the call fails as invalid input
    And the engine was not called
