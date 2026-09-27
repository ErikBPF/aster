@contract @unautomated
Feature: User working state follows the subject across containers

  Each signed-in subject has one place they left off - notebook, cell, engine -
  kept in the shared state plane (D18), so the index page, the TUI and any other
  container agree. The subject comes from the session, never from the request
  body, so a caller can only read and write their own state.

  Draft: the store rules are executed today by
  `crates/core/tests/features.rs` against `InMemoryUserState` and by the ignored
  Valkey test `state::tests::sessions_are_shared_between_connections`. Binding
  these API-level scenarios to a runner needs the server harness.

  Background:
    Given the server is running with engine "trino-local" registered

  Scenario: A subject reads back the state it wrote
    Given subject "alice" is signed in as editor
    When alice records notebook "sales" cell "c2" engine "trino-local"
    Then reading the state returns notebook "sales" and cell "c2"

  Scenario: State is private to the subject
    Given subject "alice" is signed in as editor
    And subject "bob" is signed in as editor
    When alice records notebook "sales" cell "c1" engine "trino-local"
    Then reading the state as bob returns no notebook

  Scenario: A caller with no session cannot read or write state
    When an anonymous caller reads the state
    Then the response is 403

  Scenario: A subject with nothing recorded reads an empty state
    Given subject "carol" is signed in as editor
    Then reading the state returns no notebook

  Scenario: Helper selection belongs to each user and notebook
    Given alice selected helper "lab-qwen" for notebook "sales"
    And alice selected helper "cloud" for notebook "forecast"
    And bob selected helper "cloud" for notebook "sales"
    When alice reopens notebook "sales" after visiting "forecast"
    Then alice's selected helper is "lab-qwen"
    And bob's selection and the Git notebook content are unchanged

  Scenario: A deleted selection remains visible until replaced
    Given alice selected helper "lab-qwen" for notebook "sales"
    When alice deletes helper "lab-qwen"
    Then opening notebook "sales" still shows "lab-qwen" as unavailable
    And generation does not use another helper automatically

  Scenario: A legacy last-notebook helper migrates once
    Given alice's legacy working state names notebook "sales" and helper "lab-qwen"
    When alice opens notebook "sales"
    Then the scoped selection is "lab-qwen"
    And a later legacy state write naming another helper does not replace it

  Scenario: Different notebook selections do not overwrite one another
    Given alice has notebooks "sales" and "forecast"
    When two tabs concurrently select different helpers for those notebooks
    Then each notebook retains its own selected helper
