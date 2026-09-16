# Aster behavior contract: terminal client.
#
# Status: contract-only. The equivalent Rust tests in `crates/tui` plus the server smoke path
# execute these behaviors today; binding them to a Gherkin runner is tracked in the proposal.

@contract @unautomated
Feature: Terminal client over the aster JSON API

  Background:
    Given aster-server is reachable at ASTER_SERVER
    And "alice" is an editor with a grant on engine "trino-local"
    And the notebook "sales" exists with a cell "c1"

  Scenario: The notebook list is offered at start
    When "alice" starts the terminal client
    Then the client lists the notebook "sales"
    And the client lists engine "trino-local"

  Scenario: Opening a notebook shows its cells
    When "alice" selects the notebook "sales"
    Then the cell pane shows "> c1"
    And the cell pane shows the cell SQL

  Scenario: A cell runs against the selected engine
    Given "alice" has selected engine "trino-local" for the cell
    When "alice" runs the cell
    Then the result pane shows the columns as a header row
    And the result pane shows one row per result row

  Scenario: A failing query is reported, not hidden
    Given the engine answers with an error
    When "alice" runs the cell
    Then the status line shows the server error message

  Scenario: Saving reports the revision
    When "alice" saves the notebook
    Then the status line shows "saved" and a revision prefix

  Scenario: A viewer cannot spend a query
    Given "bob" is a viewer without a grant on engine "trino-local"
    When "bob" runs the cell
    Then the status line shows a 403 from the server

  Scenario: A long value is truncated to the pane
    Given a cell returns a value wider than the result pane
    When "alice" runs the cell
    Then the rendered line ends with "…"
    And the rendered line is no wider than the pane
