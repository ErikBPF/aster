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
    And their selection references are "personal/qwen" and "shared/qwen"
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

  Scenario: Verified groups survive a session without accepting request claims
    Given Alice signs in with verified group "analysts"
    When a new session is stored and resolved by another server
    Then her principal retains group "analysts"
    And request-supplied group "admins" does not join her principal
    And an old session without groups has no group-only grant

  # V8b1 permits administrator registration only. No ordinary-user listing,
  # notebook selection, or shared generation is enabled until V8b2 proves this.
  Scenario: A registration alone does not make a shared model usable
    Given an administrator registered a shared model during V8b1
    When an ordinary user lists helpers, selects one, or requests assistance
    Then that shared model is unavailable to that user
    And its endpoint receives no request

  Scenario: Removing group membership denies the first request with an old session
    Given Alice used a shared model granted only to group "analysts"
    And Alice saved that model as her notebook selection
    And the authoritative identity source removes Alice from group "analysts"
    And Alice's existing Aster session still contains the old group
    When Alice first lists, selects, or requests that shared model after the removal
    Then it is unavailable and the request is refused before calling the model
    And an unavailable authoritative membership check refuses access the same way

  Scenario: Removing a role-grant group denies with the old derived role still in session
    Given Alice's session says editor because of a verified editor group
    And an administrator grants a shared model only to the editor role
    And the authoritative identity source removes Alice from the editor group
    When Alice first lists, selects, generates, or sends a conversation turn with that shared model
    Then the current groups recalculate Alice as a viewer for shared-model access
    And the model endpoint receives no request

  Scenario: Replacing grants has a revision and an audit trail
    Given an administrator granted group access to a shared model at revision one
    When another administrator replaces its grants using revision one
    Then the replacement has revision two and a durable actor/time record
    And a concurrent replacement using revision one conflicts without changing access

  Scenario: Shared credentials stay server-side
    Given a caller is granted a shared model
    When the caller requests assistance using that shared model
    Then the server calls the registered endpoint and model
    And no shared token is returned to the caller

  Scenario: Shared tokens are encrypted in PostgreSQL
    Given an administrator registered a shared model with a token
    When the registration is persisted and the server restarts with its application key
    Then the database contains ciphertext instead of the token
    And the granted caller can still use the shared model
    And a missing or wrong application key refuses use

  Scenario: Production shared models use administrator-approved HTTPS destinations
    Given an administrator has approved an HTTPS model endpoint
    When an administrator registers a shared model at that endpoint
    Then the registration is accepted
    And an unapproved endpoint or plain HTTP endpoint is refused

  Scenario: Revoked access cannot be bypassed using a saved selection
    Given Alice previously selected a shared model
    And her applicable shared-model grant has been removed
    When Alice requests assistance with the saved selection
    Then the request is refused before calling the model
