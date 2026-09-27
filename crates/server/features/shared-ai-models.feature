# Draft behavior contract; policy accepted by the user on 2026-09-22.
# Unbound: planned router/fake-upstream tests plus disposable Postgres checks.
@contract @unautomated
Feature: Personal helpers and administrator-managed shared models
  Personal registrations remain private. Administrators register shared models
  and grant use to existing user groups or roles.

  Scenario: Personal helpers remain private alongside shared models
    Given Alice and Bob each have a personal helper named "qwen"
    And an administrator registered a shared model named "qwen"
    And Alice is granted use of the shared model
    When Alice lists her available helpers
    Then her personal and the shared "qwen" are distinguishable
    And Bob's personal registration is not returned

  Scenario: Only administrators manage shared registrations
    Given Alice is not an administrator
    When Alice attempts to create or change a shared model registration
    Then the shared registration is not changed

  Scenario Outline: Each grant kind independently authorizes shared model use
    Given an administrator grants a shared model only to <grant>
    And a signed-in caller has verified <membership> and permission to request assistance
    When that caller requests the shared model
    Then the registered model receives the request
    And request-supplied group membership cannot expand the caller's grants

    Examples:
      | grant            | membership                  |
      | group "analysts" | membership of "analysts"    |
      | role "editor"    | the role "editor"           |

  # The revocation/refresh deadline is pending in the plan; this example covers
  # the invariant once verified identity has refreshed, not an immediate SLA.
  Scenario: Removing group membership denies use after verified identity refresh
    Given Alice used a shared model granted only to group "analysts"
    And Alice has been removed from group "analysts"
    And Alice's current verified authorization state reflects that removal
    When Alice requests the shared model using her saved selection
    Then the request is refused before calling the model

  Scenario: Shared credentials stay server-side
    Given a caller is granted a shared model
    When the caller requests assistance using that shared model
    Then the server calls the registered endpoint and model
    And no shared token is returned to the caller

  Scenario: Revoked access cannot be bypassed using a saved selection
    Given Alice previously selected a shared model
    And her applicable shared-model grant has been removed
    When Alice requests assistance with the saved selection
    Then the request is refused before calling the model
