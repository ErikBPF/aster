# Executable integration coverage: notebook_isolation.rs and store.rs tests
# selected by just notebook-team-target-validation; not Cucumber-bound.
@unautomated
Feature: Approved repositories for durable workspaces
  Repository registration preserves legacy team targets and does not activate a checkout.

  Scenario: A maintainer registers two repositories independently
    Given Alice has current member and maintainer groups for team Alpha
    And its GitHub App policy allows repositories analytics and reports
    When Alice registers both verified repositories
    Then both are listed with independent versions
    And changing analytics does not change reports
    And stale updates are refused

  Scenario: Current membership controls registration and listing
    Given Alice has registered an approved repository
    When her maintainer group is removed
    Then she can list the repository but cannot change its registration
    When her member group is removed
    Then she cannot list the team's repository registrations
