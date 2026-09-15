# Contract-only (@unautomated). The equivalent behavior is exercised by
# crates/core text-format tests and the crates/server GitNotebookStore tests,
# plus the API smoke path in the justfile. Binding these steps to a Gherkin
# runner remains tracked in the proposal (IP S2+).
@contract @unautomated
Feature: Git-backed notebooks
  Notebooks are text-first files committed on a branch-per-session, so a save
  is a git commit and the metadata database stores only the index.

  Background:
    Given the server is running with the notebook directory on a git repository
    And the notebook branch is "session/alice"

  Scenario: Saving a notebook creates a commit on the session branch
    When subject "alice" saves notebook "sales" with one cell "SELECT 1"
    Then the response carries a non-empty revision
    And the current branch is "session/alice"

  Scenario: A saved notebook is read back with the same cells
    Given subject "alice" has saved notebook "sales" with one cell "SELECT 1"
    When subject "alice" opens notebook "sales"
    Then the notebook has one cell whose SQL is "SELECT 1"

  Scenario: Listing notebooks returns the saved ids
    Given subject "alice" has saved notebook "sales"
    When subject "alice" lists notebooks
    Then the list contains "sales"

  Scenario: A caller without identity cannot save a notebook
    When an unidentified caller saves notebook "sales"
    Then the response status is 403

  Scenario: A viewer cannot save a notebook
    When subject "bob" with role "viewer" saves notebook "sales"
    Then the response status is 403

  Scenario: A notebook id with a path separator is rejected
    When subject "alice" saves notebook "../evil"
    Then the response status is 400
