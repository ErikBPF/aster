# V2a behavior contract, bound to crates/server/tests/contracts.rs.
@contract @notebook-ownership
Feature: Legacy notebook ownership and stale-write protection

  Scenario: Unassigned legacy content stays read-only
    Given a notebook is committed on the legacy source branch without an owner
    When an editor saves a change with the current content revision
    Then the save is refused without changing the file, index or branch revision

  Scenario: An administrator assigns and corrects an exact owner
    Given a committed legacy notebook has a known content revision
    When a verified administrator assigns an exact owner against that revision
    Then the assignment is recorded without changing Git
    And an ordinary editor cannot assign or correct the owner
    When that administrator corrects a mistaken owner with the current owner and revision
    Then the old owner loses write access immediately
    And the correction reason and administrator are recorded

  Scenario: A stale tab cannot overwrite a newer notebook blob
    Given Alice owns a notebook and two tabs loaded the same content revision
    When the first tab saves a change
    Then the second tab's save is refused as a conflict
    And the first tab's committed content remains readable

  Scenario: Unrelated notebook commits do not make this one stale
    Given Alice loaded notebook sales with its content revision
    When another notebook is committed before Alice saves sales
    Then Alice can save sales with her original content revision

  Scenario: Creation requires an absence precondition
    Given no committed notebook has the requested ID
    When Alice creates it without an explicit absence precondition
    Then the create is refused before a Git write

  Scenario: Owner assignment and create serialize across Git and metadata
    Given Alice is creating a notebook while an admin assigns legacy ownership
    When both operations target the same notebook ID
    Then one serial order determines the owner
    And no caller commits under another owner's assignment
