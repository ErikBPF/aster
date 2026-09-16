# Behavior contract: git-backed notebooks.
# Status: bound to `crates/server/tests/contracts.rs`, which drives the router
# against a temporary real git repository; the unit-level store tests stay in
# crates/server/src/gitstore.rs.
@contract
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

  Scenario: A notebook id with a percent-encoded path separator is rejected
    # A raw "../" is normalized away by the HTTP layer before routing, so the
    # percent-encoded form is what actually reaches the id guard.
    When subject "alice" saves notebook "..%2Fevil"
    Then the response status is 400
