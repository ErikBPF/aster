# Acceptance examples from the 2026-11-10 Git/web planning conversation.
# Gherkin steps remain unbound. Executable RED/GREEN coverage runs through
# just workspace-e2e-validation: workspace.rs, notebook_isolation.rs, real TLS
# Git/PostgreSQL/RustFS fixtures and workspace-live-browser.py.
# See the stage receipts for observed behavior; this file is not an executed test.
@contract @unautomated
Feature: Recoverable multi-repository notebook workspaces
  A user works on one active branch per repository checkout.
  Draft persistence, Git commits and remote publication are separate actions.

  Scenario: Two authorized repositories retain their own notebook branches
    Given Alice may use team Alpha repositories "analytics" and "reports"
    When Alice clones both into her workspace
    And creates notebook "sales" on branch "alice/analysis" in "analytics"
    And creates notebook "sales" on branch "alice/reports" in "reports"
    Then each notebook shows its own repository and active branch
    And editing one notebook does not change the other repository

  Scenario: Notebooks in one checkout share its active branch
    Given Alice has selected branch "alice/analysis" in checkout "analytics"
    When Alice creates notebooks "sales" and "costs"
    Then both notebooks belong to branch "alice/analysis" in checkout "analytics"
    And neither notebook is created outside an authorized branch

  Scenario: Autosave does not publish or commit a draft
    Given Alice has a notebook on an authorized working branch
    When Alice edits its SQL and autosave completes
    Then her draft can be reopened with the edited SQL
    And the branch commit and GitHub branch are unchanged
    When Alice explicitly commits the notebook
    Then its edited SQL is in a local commit
    And GitHub remains unchanged until Alice explicitly pushes

  Scenario: A recovered checkout includes unpublished work
    Given Alice has an object-storage checkpoint of her workspace
    And it contains a local commit, an uncommitted notebook edit and an untracked file
    When the local checkout is lost and Alice restores the workspace
    Then the same active branch and local commit are available
    And the uncommitted edit and untracked file retain their content
    And the unpublished work is not reported as pushed

  Scenario: Pull preserves divergent local work
    Given Alice's branch and its upstream each contain a different new commit
    When Alice requests Pull
    Then Aster reports that external resolution is required
    And her local branch and notebook contents are unchanged

  Scenario: Clean Pull refreshes the notebook
    Given Alice's checkout and editor have no pending changes
    And its upstream contains every local commit and a new notebook change
    When Alice requests Pull
    Then the branch fast-forwards to the upstream commit
    And the notebook shows the updated content and content revision

  Scenario: The user opens a pull request explicitly
    Given Alice has pushed her authorized branch to its team-approved repository
    When Alice requests Open PR against its approved base branch
    Then Aster displays a pull request for that repository, head and base
    And the base branch is not changed by creating the pull request

  Scenario: App access does not grant cross-user checkout access
    Given Alice and Bob have separate workspaces in the same team repository
    When Bob attempts to open, restore or push Alice's checkout through Aster
    Then Aster refuses each operation
    And Alice's files, Git refs and recovery checkpoints are unchanged

  Scenario: Initial checkout uses shallow history
    Given Alice may clone a team-approved repository whose history exceeds the initial clone depth
    And the configured base is available within that initial depth
    When Aster creates her initial checkout
    Then her checkout has shallow Git history
    And its active branch starts at the verified configured base commit

  Scenario: A base outside initial shallow history is not silently replaced
    Given the configured base commit is older than the initial clone depth
    When Alice creates a checkout
    Then Aster either obtains and verifies that exact base within its limits or reports failure
    And it never substitutes the latest default-branch commit as the requested base

  Scenario: Pull preserves uncommitted notebook changes
    Given Alice has an uncommitted notebook draft
    And the upstream branch has advanced
    When Alice requests Pull
    Then Aster refuses to replace her draft
    And the local branch and draft contents are unchanged

  Scenario: A failed checkpoint preserves the previous recoverable workspace
    Given Alice has a successful checkpoint containing notebook SQL "SELECT 1"
    And a new checkpoint containing "SELECT 2" fails to become durable
    When Alice restores the last successful checkpoint
    Then notebook SQL is "SELECT 1"
    And Aster does not report the failed checkpoint as recoverable

  Scenario: Retrying Open PR returns the existing matching request
    Given Alice has an open pull request for her repository, branch and approved base
    When Alice repeats Open PR for that same repository, branch and base
    Then Aster returns the existing pull request
    And it does not create a duplicate pull request
