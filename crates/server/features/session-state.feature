@contract @unautomated
Feature: Session and handshake state live in a shared store

  Sessions are opaque ids resolved on every request in a store every container
  shares, so a session survives a restart of the pod that created it and a
  revoked id is refused everywhere (D20). Login attempts are held in the same
  store and redeemed exactly once.

  Draft: the store port and its expiry/revocation rules are executed today by
  `crates/core/tests/features.rs` against `InMemorySessions`/`InMemoryHandshakes`.
  The cross-container scenarios below are executed by the ignored Valkey
  integration test `state::tests::sessions_are_shared_between_connections`
  (`cargo test -p aster-server -- --ignored` with a Valkey at
  `ASTER_TEST_STATE_URL`) and were confirmed by hand: a session minted in Valkey
  by another client was accepted by the server (200 on `/api/notebooks`) and the
  read refreshed `last_seen` and the key TTL. Binding these scenarios to a
  Gherkin runner against a real Valkey remains open.

  Background:
    Given the server is running with engine "trino-local" registered

  Scenario: A session created by one container is accepted by another
    Given a session for subject "alice" created through container "a"
    When the client sends that session cookie to container "b"
    Then the response is 200

  Scenario: An idle session expires even if its cookie is still presented
    Given a session last used more than ASTER_SESSION_TTL_SECONDS ago
    When the client requests "/api/notebooks" with its cookie
    Then the response is 403

  Scenario: A session revoked on one container is refused on another
    Given a session for subject "alice" created through container "a"
    When that session is revoked through container "b"
    And the client sends that session cookie to container "a"
    Then the response is 403

  Scenario: A login attempt is redeemed exactly once
    Given a handshake payload stored for state "s1"
    When the callback for state "s1" is exchanged successfully
    And the callback for state "s1" is replayed
    Then the replay is refused with 403

  Scenario: An expired login attempt is refused
    Given a handshake payload stored for state "s1" more than ASTER_HANDSHAKE_TTL_SECONDS ago
    When the callback for state "s1" is exchanged
    Then the response is 403

  Scenario: An unreadable session store fails closed
    Given the session store is unreachable
    When the client requests "/api/notebooks" with a session cookie
    Then the response is 403
    And the dev identity seam is not used as a fallback
