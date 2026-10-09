# Bound by team_git_boot_tests::authentik_catalog_team_boot_without_github_or_shared_models.
Feature: Authentik catalog team registration
  Scenario: Catalog teams do not require GitHub or a shared model
    Given OIDC with a signed user UUID and a real Authentik current membership reader
    And a server-owned team policy mapping team names to stable Authentik group UUIDs
    When Aster starts without GitHub credentials or shared models
    Then the configured team admits a member and refuses a nonmember
    And GitHub targets and shared models remain disabled
