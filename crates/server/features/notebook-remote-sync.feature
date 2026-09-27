# Draft scope contract; remote GitHub synchronization requested on 2026-09-22.
# Unbound Gherkin: eight local bare-remote route tests and three focused Git
# unit tests run separately; a named GitHub E2E target is still required.
# Save is local; explicit Sync pushes an owner-authorized branch. Conflict UX
# and each team's named GitHub target remain in the plan.
@contract @unautomated
Feature: GitHub-backed notebook synchronization
  Each team configures its GitHub repository and default branch; that team's
  repository holds its members' personal/session branches.
  Remote synchronization is distinct from a local commit.

  Scenario: A personal branch starts at the team default branch
    Given Alice has verified membership in a team with a configured repository and default branch
    And Alice has no personal branch in that repository
    When Alice starts a notebook session
    Then Aster creates her authorized branch from the team default branch
    And another user cannot sync that branch through Aster

  Scenario: Promotion requires a separate pull request
    Given Alice synchronized her personal branch
    When Alice saves and synchronizes another notebook change
    Then the team default branch is unchanged
    And promotion requires a separate pull request

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

  Scenario: Team configuration selects the repository and base branch
    Given team Alpha and team Beta configure different GitHub repositories and default branches
    And Alice has verified membership in team Alpha only
    When Alice starts a session and synchronizes her personal branch
    Then Aster creates that branch from team Alpha's configured default branch
    And only team Alpha's configured repository changes

  Scenario: Two teams cannot claim the same numeric repository target
    Given Alpha and Beta policy both name the same GitHub repository ID
    When Aster validates the team Git policy before startup
    Then it refuses the duplicate target before any workspace or Sync is active

  Scenario: Only a designated team maintainer changes its Git target
    Given Alice is a verified maintainer of team Alpha
    And Bob is an ordinary member of team Alpha
    When Alice configures team Alpha's repository and default branch
    Then that configuration is saved for team Alpha
    When Bob attempts to change team Alpha's Git target
    Then Aster refuses without changing the configuration

  Scenario: Another team's maintainer cannot change this team's Git target
    Given Carol is a verified maintainer of team Beta only
    When Carol attempts to change team Alpha's Git target
    Then Aster refuses without changing the configuration

  Scenario: Unknown team membership cannot select a GitHub target
    Given the caller has no verified membership in a configured team
    When the caller requests notebook synchronization with a team or repository parameter
    Then Aster refuses without choosing a default repository
    And no GitHub write is made

  Scenario: Application installation access does not authorize another user's branch
    Given the GitHub application can write the shared team repository
    And Alice is not authorized to write Bob's personal session branch
    When Alice requests synchronization to Bob's branch
    Then no GitHub write is made for that request

  Scenario: The installation token is limited to one verified team repository
    Given team Alpha's target records a verified installation and numeric repository ID
    When Aster requests an installation token for Alice's authorized branch
    Then the request names only that repository ID and contents write permission
    And a token for another installation or repository is never used for Sync

  Scenario: GitHub credentials stay out of notebook and process artifacts
    Given Aster receives a short-lived installation token for a verified team target
    When Aster fetches and pushes the authorized branch over HTTPS
    Then no token appears in process arguments, local Git config, notebook files, or logs
    And an authentication redirect to another host is refused

  Scenario: A stale or unverified team target cannot be activated
    Given a designated maintainer has a verified team group UUID
    And the team's current target has a recorded version and repository ID
    When a caller with only a matching group path or stale target version changes it
    Then Aster refuses without changing the target or audit trail
    And a repository transfer or installation change requires fresh verification

  Scenario: A remote branch change during Sync never rewrites history
    Given Alice has a local notebook commit on her authorized session branch
    When the remote ref is created, deleted, or advanced during Sync
    Then Aster refuses a stale ref update and retains the local commit
    And no non-fast-forward remote update occurs

  Scenario: A crash after push cannot conceal a deleted remote branch
    Given Aster durably recorded an intent for Alice's team repository, ref, and local commit
    When the push succeeds but Aster crashes before its success response
    And the remote branch is deleted before Alice retries
    Then Aster refuses to recreate the branch silently
    And the local commit remains available for explicit recovery

  Scenario: A clean remote advance refreshes local content before editing
    Given the intended remote branch contains every local commit
    And the editor and checkout have no unsaved changes or unresolved Sync intent
    When Alice synchronizes after another authorized remote commit
    Then Aster fast-forwards the local branch and refreshes notebook revisions
    And a dirty editor instead receives a conflict without replacement
