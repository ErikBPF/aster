# Draft scope contract; remote GitHub synchronization requested on 2026-09-22.
# Unbound: planned bare-remote tests and separate named GitHub E2E target.
# Save triggers, branch lifecycle and conflict UX remain decisions in the plan.
@contract @unautomated
Feature: GitHub-backed notebook synchronization
  A shared team GitHub repository holds personal/session branches.
  Remote synchronization is distinct from a local commit.

  Scenario: A remotely synchronized notebook is independently recoverable
    Given a caller is authorized for a configured notebook repository and branch
    When the caller saves and successfully synchronizes a notebook
    Then an independent checkout of the intended GitHub branch has the same notebook content
    And the reported synchronized revision matches the remote commit

  Scenario: Remote failure does not masquerade as synchronized success
    Given a notebook has been committed locally
    And its GitHub remote cannot be updated
    When synchronization is attempted
    Then the notebook is not reported as synchronized
    And its local committed content remains recoverable

  Scenario: Personal sessions do not overwrite each other's branches
    Given Alice and Bob use the same team notebook repository
    And each session has a distinct authorized branch
    When Alice and Bob save and synchronize different content for notebook "sales"
    Then each branch contains its session's own notebook content
    And neither synchronization changes the other branch

  Scenario: Application installation access does not authorize another user's branch
    Given the GitHub application can write the shared team repository
    And Alice is not authorized to write Bob's personal session branch
    When Alice requests synchronization to Bob's branch
    Then no GitHub write is made for that request
