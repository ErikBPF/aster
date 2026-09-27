# V2b behavior contract. Dedicated router tests cover part of this draft;
# bind these steps before treating the feature as an automated contract.
# Local bare repositories are fixtures, not live GitHub synchronization.
@contract @unautomated
Feature: Team-bound notebook workspaces

  Scenario: Two teams configure separate targets and default ancestry
    Given Alpha and Beta have different allowed repositories and default branches
    And Alice is a verified maintainer and member of Alpha only
    And Carol is a verified maintainer and member of Beta only
    When each maintainer configures that team's target
    And each creates notebook sales in their own authenticated session workspace
    Then each session branch starts from its team's configured default commit
    And the two sales notebooks have independent content and commits

  Scenario: Two sessions of one subject cannot overwrite each other
    Given Alice is a verified member of Alpha with two authenticated sessions
    When each session creates notebook sales with different SQL
    Then each session rereads only its own sales content
    And each session has a different server-derived branch

  Scenario: An explicit personal workspace persists across sessions
    Given Alice has two verified authenticated sessions in Alpha
    When Alice saves notebook sales in her personal workspace from the first session
    Then the second session sees the same personal branch content only when selecting Personal
    And its default session workspace does not silently adopt that content

  Scenario: Only a verified team maintainer changes that team's target
    Given Alice is a verified maintainer and member of Alpha
    And Bob is a verified ordinary member of Alpha
    And Carol is a verified maintainer of Beta only
    When Bob or Carol tries to change Alpha's target
    Then each request is refused without changing Alpha's target

  Scenario: Team selection never grants membership or branch authority
    Given Alice is a verified member of Alpha only
    When Alice selects Beta or supplies Bob's branch in a notebook request
    Then Aster refuses before a Git write
    And Beta's repository and Bob's branch remain unchanged

  Scenario: Forged groups and unrecognized teams fail closed
    Given a caller has no verified team membership
    When the caller supplies group, team and repository headers
    Then Aster refuses without selecting a default repository
    And no branch is created

  Scenario: Removing current UUID membership revokes an active workspace session
    Given Alice has an active verified session with a cached Alpha group path
    And the current identity provider confirms Alice's Alpha member UUID
    When Alice is removed from that current group while her session remains active
    Then Alpha notebook read, save, recovery, Sync, conversation and helper requests are refused
    And a team-qualified Connect conversation request is refused before a helper call

  Scenario: Current identity without a UUID team policy cannot use cached claims
    Given Alice has an active verified session with a cached Alpha group path
    And a current identity provider is configured without an Alpha UUID team policy
    When Alice reads an Alpha team notebook
    Then Aster refuses before selecting the workspace checkout

  Scenario: Unqualified notebook reads do not fall back to the legacy checkout
    Given a legacy notebook exists and team workspaces are active
    When Alice requests the unqualified REST or Connect notebook list or read route
    Then Aster refuses before exposing legacy notebook IDs or content

  Scenario: Legacy ownership is not silently moved to a team
    Given a notebook is committed in the V2a legacy checkout without an owner
    When Alice opens a team workspace with the same notebook ID
    Then the legacy notebook remains unassigned and unchanged
    And any copy into the team workspace requires explicit owner migration

  Scenario: Target reconfiguration does not retarget an unsynced workspace
    Given Alice's session branch was seeded from Alpha's configured default commit
    And Alice has an unsynced notebook commit
    When Alpha's maintainer changes the repository and default branch
    And the server reopens Alice's existing workspace
    Then Alice's branch remains bound to its original repository, commit and target version
    And a new session starts only from the new target

  Scenario: Moving the upstream default does not change a pinned branch root
    Given Alpha's configured target records one verified default commit
    And the upstream default branch advances before Alice's first notebook save
    When Alice creates her session workspace
    Then its first commit descends from the recorded default commit

  Scenario: Git refs and checkout paths do not expose session credentials
    Given Alice has a server-issued bearer session cookie
    When Alice saves in her session workspace
    Then no Git ref or checkout path contains the bearer cookie value

  Scenario: Expired session work needs explicit same-owner recovery
    Given Alice saved an unsynced notebook in an expired session branch
    When Alice signs in with a new verified session
    Then the new session does not silently adopt the old notebook
    And Alice can inspect the old notebook through an explicit recovery path
    And Bob cannot inspect Alice's recovery branch

  @unautomated
  Scenario: Conversation memory follows the server-derived workspace
    Given Alice has the same notebook ID in Alpha, Beta and two Alpha sessions
    When Alice opens each notebook's conversation
    Then each team and session has a distinct private transcript
    And no request-supplied branch or workspace key can select another transcript

  @unautomated
  Scenario: Helper choice follows the server-derived workspace
    Given Alice registered two personal helpers and has the same notebook ID in two teams
    When Alice selects different helpers for each team and Alpha session
    Then each notebook workspace retains only its own helper choice
    And a legacy unqualified helper choice cannot leak into a team workspace

  @unautomated
  Scenario: Connect chat and helper choice use the same private workspace as REST
    Given Alice has notebook base in Alpha and Beta session workspaces
    When Alice sends a Connect chat turn for Alpha using her current session
    And Alice selects a Connect helper for that workspace
    Then Alpha REST reads the new conversation and helper choice
    And Beta REST reads neither Alpha's turn nor its helper choice

  @unautomated
  Scenario: Denied Connect chat never reaches the helper
    Given Alice has notebook base in Alpha and an available helper
    When a foreign member, revoked session, or missing notebook requests a Connect turn
    Then Aster refuses before calling the helper or appending a turn

  Scenario: Browser notebook actions stay in the selected team workspace
    Given Alice has notebook base in Alpha's session and personal workspaces
    When Alice opens the team notebook page and switches between Session and Personal
    Then the page identifies the selected workspace without accepting a Git branch from the browser
    And Save commits locally while a separate Sync action targets that workspace
    And chat and helper choice use the same team and workspace context
