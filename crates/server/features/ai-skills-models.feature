@contract @unautomated
# Behavior contract; no executable Gherkin bindings. API/browser regression
# coverage and observed RED/GREEN receipts are recorded in docs/plans.
Feature: Versioned AI skills and controlled model registration

  Scenario: A user registers a non-executable skill from the AI tab
    Given Alice is signed in
    When Alice registers skill "sql-review" with instructions and reference files
    Then the AI tab lists the registered skill and its version
    And registering or using the skill does not execute bundled code

  Scenario: Conversations keep the selected skill version
    Given Alice selected version "1" of skill "sql-review" for a conversation
    When version "2" of the skill is published
    Then the conversation still selects version "1"
    When Alice explicitly upgrades that selection to version "2"
    Then subsequent assistance selects version "2"

  Scenario: Ordinary users cannot publish organization-wide skills
    Given Alice is not an administrator
    When Alice attempts to share a skill with the whole organization
    Then organization-wide publication is refused

  Scenario: Company model ACLs govern use
    Given an administrator registered company model "company-sql"
    And Alice has no applicable grant for that model
    When Alice attempts assistance with "company-sql"
    Then the model endpoint receives no request

  Scenario: Personal registration requires an administrator grant
    Given Alice has no personal-model registration permission
    When Alice attempts to register a model with a name, URL and token
    Then registration is refused
    When an administrator grants Alice personal-model registration permission
    And Alice registers a model with a name, valid URL and token
    Then Alice sees her registration in the AI tab
    And the token is not returned in the registration response

  Scenario: Administrators can publish organization-wide skills
    Given an administrator has registered a skill
    When the administrator shares it with the whole organization
    Then the skill is available to that organization's users
    And its sharing scope is shown in the AI tab

  Scenario: Team sharing grants use but not editing
    Given Alice and Bob belong to team "analytics"
    And Eve does not belong to team "analytics"
    When Alice shares her skill "sql-review" with team "analytics"
    Then Bob can read and select the skill
    And Bob cannot modify Alice's skill
    And Eve cannot read the skill or its versions

  Scenario: Revoked personal-model permission blocks saved use
    Given Alice registered a personal model while permitted
    And Alice selected it in a conversation
    When an administrator revokes her personal-model permission
    Then subsequent registration and updates are refused
    And subsequent assistance with that saved model makes no upstream request
    And the existing registration is retained for recovery
    And Alice can delete her own registration

  Scenario: Skill version pins do not preserve revoked access
    Given Bob selected version "1" of Alice's team skill in a conversation
    And the conversation already used the skill
    When Bob loses access to that skill
    Then his next conversation turn is refused before an upstream request
    And the response explains how to start a new session without that skill

  Scenario: A portable skill import remains an unsaved private draft
    Given Alice has a local JSON package with a SKILL.md and nested text references
    When Alice imports the package in the AI workspace
    Then she can review its name, description and files before saving
    And package ownership, sharing and revision counters are not imported
    And unsafe paths and file-directory collisions are refused

  Scenario: Loading a version cannot overwrite newer editor changes
    Given Alice is editing a skill while a version load is pending
    When Alice changes the instructions before the response arrives
    Then her newer instructions remain in the editor

  Scenario: Export uses the selected saved version
    Given Alice selected version "1" and has unsaved edits
    When Alice exports the selected version
    Then the package contains saved version "1" content
    And it does not contain her unsaved edits or access grants

  Scenario: Skill library exposes explicit management actions
    Given an owned skill with multiple immutable versions
    When the owner opens the skill library
    Then Edit opens the current editable version
    And Edit includes saved version history and opens historical versions read-only
    And Permissions opens the current skill sharing controls
    And a user without edit permission cannot use Edit

  Scenario: Edit manages content, reference assets and version history together
    Given an owned skill with saved versions
    When the owner chooses Edit
    Then content, reference files, version history and Delete are available on that screen
    And a reference file has a file name, an optional folder and a package-path preview
    And a new reference file defaults to the references folder
    And existing root and nested package paths are preserved

  Scenario: Confirmed deletion revokes every skill version
    Given an owned skill with multiple versions
    When the owner confirms Delete with current content and sharing revisions
    Then all versions disappear from the library and cannot be resolved for future turns
    And existing conversation messages remain unchanged
    And conversations that used the skill require a new session before further turns
    And the deleted identifier cannot be reused
    But a non-owner, stale editor or cancelled confirmation cannot delete the skill

  Scenario: File navigation reflects package folders without numbered field labels
    Given a skill contains files in nested folders including matching basenames
    When the owner opens Edit
    Then the file navigator groups folders and displays each file by its basename
    And full package paths distinguish files for assistive technology
    And File name, Folder and File content labels have no numeric suffix
    And Add file is inside the navigator
    And saving preserves the selected file and its draft content
    And file selection retains keyboard focus and the layout fits a narrow screen

  Scenario: Import validates text and metadata before replacing an unsaved draft
    Given a user has unsaved skill edits
    When an import contains malformed UTF-8, invalid Unicode, whitespace-only name or oversized metadata
    Then import reports a validation error and preserves the draft
    And metadata limits are measured in UTF-8 bytes like the server

  Scenario: Export waits for the requested saved version
    Given a different skill version is still loading
    Then Export version is disabled until the selected version is available
    And the exported JSON contains that saved version rather than unsaved edits

  Scenario: An exported package imports as a new private skill
    Given a saved historical skill version with nested reference files
    When the user exports that version and imports its JSON
    Then import creates an unsaved private draft without ownership, grants or source identity
    And explicitly saving creates a different skill at version one
    And its instructions and reference files remain available after restart

  Scenario: Personal defaults initialize new conversations without upgrading pins
    Given Alice enabled two exact skill versions in her personal defaults
    When Alice starts a notebook or cell conversation
    Then both selected versions supply instructions to the model
    And Bob does not inherit Alice's defaults
    And changing defaults does not change existing conversation selections
    And explicitly switching all skills off stops new instruction injection

  Scenario: Enabled notebook MCP tools respect confirmation and conversation boundaries
    Given no MCP connection is enabled in a new conversation
    When Alice enables an authorized connection
    Then only that connection's tools are offered to the model
    And admin-approved reads can execute without another confirmation
    But a changing action waits for confirmation showing its exact arguments
    And concurrent or repeated confirmations dispatch the action at most once
    And confirmation cannot move to another user, notebook, cell or session
    And interrupted actions are never automatically retried
    And revoked historical MCP access blocks later model requests

  Scenario: Conversation controls leave room for the message
    Given a notebook or cell conversation has available skills and tools
    Then the message and Send remain visible in the composer
    And Attach, Skills and Tools open compact popovers with active counts
    And skills and tools can be searched without changing their selection
    And Escape closes a popover and returns focus without closing the chat
    And attached context can be reviewed and removed before sending
    And exact skill versions and tool confirmations retain their existing behavior

  Scenario: Isolated mock assistant uses a selected skill before explicit query execution
    Given the isolated development mock assistant is enabled
    When a user selects the demo skill and requests an orders query
    Then the assistant receives versioned instructions and reference content
    And no query runs until the user inserts the suggestion and chooses Run
    And the mock engine returns synthetic rows
