# Behavior contract: local Git save transaction.
# Status: draft Gherkin; executable regressions live in gitstore.rs unit tests.
@unautomated
Feature: A local notebook save is one committed change

  Scenario: Repeating an unchanged save keeps the revision
    Given a notebook was committed on a session branch
    When the same notebook is saved again
    Then the revision and commit count are unchanged

  Scenario: An unchanged save repairs local drift
    Given a committed notebook has a different working file
    When the committed content is saved again
    Then the revision is unchanged and the working file matches the commit

  Scenario: A rejected commit preserves readable content
    Given a notebook was committed on a session branch
    When a later save fails to commit
    Then the original notebook remains readable
    And the branch revision is unchanged

  Scenario: Concurrent saves do not include another staged file
    Given an unrelated file is staged in the notebook checkout
    When two distinct notebooks are saved concurrently
    Then each returned revision changes only its own notebook
    And the unrelated file remains staged

  Scenario: A staged version of the target notebook is preserved
    Given another tool staged the target notebook
    When a save is attempted
    Then the save is refused without changing the staged or working version

  Scenario: Checkout hooks do not run
    Given an existing notebook checkout has executable Git hooks
    When Aster opens the checkout and saves a notebook
    Then neither checkout nor commit hooks execute

  Scenario: A notebook path cannot write outside its checkout
    Given a notebook file points to data outside the checkout
    When Aster attempts to save that notebook
    Then the outside data remains unchanged

  Scenario: A second process cannot initialize an owned checkout
    Given a process holds the Git administrative checkout lock
    When another process opens the checkout before Git initialization
    Then the open is refused without initializing Git
    And a later open succeeds after the owner releases the lock

  Scenario: Clones retain checkout ownership across processes
    Given two in-process clones share a notebook checkout
    When the original store closes and another process opens a different branch
    Then the open is refused without switching branches
    And the control file is outside the tracked worktree
    And the other process can open the checkout after the final clone closes

  Scenario: Git cleanup cannot replace the lock inode
    Given Aster owns a notebook checkout
    When Git removes ignored and untracked working files
    Then another process still cannot open the checkout

  Scenario: A linked Git worktree uses its administrative lock directory
    Given a notebook checkout is a linked Git worktree
    When Aster opens that checkout
    Then its control file is in that worktree's Git administrative directory
