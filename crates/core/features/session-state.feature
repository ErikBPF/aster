@contract
Feature: Shared session and handshake state

  Sessions are opaque ids looked up in a store that every container shares
  (D20), so any process can resolve, revoke and list the same session. An
  unknown, revoked or idle id resolves to nothing and the caller is refused; a
  handshake payload can be redeemed exactly once.

  Scenario: A new session resolves to the signed-in subject
    Given a session store with a 3600 second idle timeout
    When a session is created for "alice" with roles "editor" at time 1000
    Then the session resolves to subject "alice"
    And the session id reveals nothing about the subject

  Scenario: A session that was never used for longer than the timeout is refused
    Given a session store with a 60 second idle timeout
    And a session created for "alice" with roles "editor" at time 1000
    When that session is resolved at time 1061
    Then no session is resolved

  Scenario: Use refreshes the idle deadline
    Given a session store with a 60 second idle timeout
    And a session created for "alice" with roles "editor" at time 1000
    When that session is resolved at time 1059
    Then the session resolves to subject "alice"
    When that session is resolved at time 1100
    Then the session resolves to subject "alice"

  Scenario: A revoked session is refused
    Given a session store with a 3600 second idle timeout
    And a session created for "alice" with roles "editor" at time 1000
    When that session is revoked
    And that session is resolved at time 1001
    Then no session is resolved

  Scenario: Sessions are listed per subject
    Given a session store with a 3600 second idle timeout
    And a session created for "alice" with roles "editor" at time 1000
    And a session created for "alice" with roles "viewer" at time 1001
    And a session created for "bob" with roles "admin" at time 1002
    Then subject "alice" has 2 sessions listed
    And subject "bob" has 1 sessions listed

  Scenario: A handshake payload is redeemed exactly once
    Given a handshake store with a 300 second timeout
    When the handshake payload "verifier-1" is stored for state "state-1" at time 1000
    Then redeeming state "state-1" at time 1001 yields "verifier-1"
    And redeeming state "state-1" at time 1002 yields nothing

  Scenario: An expired handshake is not redeemed
    Given a handshake store with a 300 second timeout
    When the handshake payload "verifier-2" is stored for state "state-2" at time 1000
    Then redeeming state "state-2" at time 1301 yields nothing
