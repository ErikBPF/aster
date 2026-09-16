@contract
Feature: Working state per subject

  Where a user left off - the notebook, the cell and the engine they were on -
  is kept per subject in the shared state plane, so any container resumes the
  same place (D18). The state plane stores it; the clients agree on its shape.

  Background:
    Given an empty user state store

  Scenario: State is remembered for the subject that recorded it
    When subject "alice" records notebook "sales" cell "c2" engine "trino-local"
    Then subject "alice" resumes notebook "sales"
    And subject "alice" resumes cell "c2"
    And subject "alice" resumes engine "trino-local"

  Scenario: State is per subject, not global
    When subject "alice" records notebook "sales" cell "c1" engine "trino-local"
    And subject "bob" records notebook "ops" cell "c9" engine "trino-local"
    Then subject "bob" resumes notebook "ops"
    And subject "alice" resumes notebook "sales"

  Scenario: A subject with no recorded state resumes nothing
    Then subject "carol" has no recorded state
