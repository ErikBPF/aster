# Bound by tests/local-stack-context.py via just local-stack-context-check and ci.
# Exercises real recipe dispatch with fake clients, not a running cluster.
@contract
Feature: Local stack commands keep their checked target and report failures

  Scenario: Refuse an unintended or unavailable current context
    Given the selected local context differs from the current context or the query fails
    When a local stack install, build, deploy, teardown or status command runs
    Then it fails before invoking a cluster or container operation
    And an empty selected context is refused

  Scenario: Keep the checked context after ambient state changes
    Given the current context matches the selected local context
    When local chart or stack commands run and the ambient context changes after validation
    Then every cluster client receives the selected context explicitly
    And a custom context and DevSpace profile remain supported

  Scenario: Bootstrap deploys into its selected local context
    Given the named disposable profile is already running
    When the bootstrap command selects its context and loads the image
    Then its deployment and status commands receive the selected context explicitly

  Scenario: Report client failures without breaking idempotent chart teardown
    Given the selected local context is available
    When a deployment or teardown client fails
    Then the recipe fails
    But removing an already absent Helm release succeeds
