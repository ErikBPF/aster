@contract @unautomated
Feature: Sign in through the identity provider

  The server authenticates callers with OpenID Connect, or with the dev
  identity seam while no provider is configured.

  Background:
    Given the server is running with engine "trino-local" registered

  Scenario: An unidentified browser is sent to the login route
    Given no session cookie
    When the browser requests "/"
    Then the response is a redirect to "/login"

  Scenario: Login falls back to the dev form while no provider is configured
    Given the server runs without ASTER_OIDC_ISSUER
    When the browser requests "/login"
    Then the response is a redirect to "/dev-login"

  Scenario: A callback without a handshake cookie is refused
    Given no aster_oidc cookie
    When the browser requests "/callback?code=x&state=y"
    Then the response is 403

  Scenario: A callback whose state does not match the handshake is refused
    Given a signed handshake cookie holding state "s1"
    When the browser requests "/callback?code=x&state=s2"
    Then the response is 403

  Scenario: A signed session cookie identifies the caller
    Given a session cookie for subject "alice" with role "editor"
    When the client requests "/api/notebooks"
    Then the response is 200

  Scenario: A tampered session cookie is refused
    Given a session cookie whose payload was modified
    When the client requests "/api/notebooks"
    Then the response is 403

  Scenario: The dev identity seam is refused once a provider is configured
    Given the server runs with ASTER_OIDC_ISSUER set
    When the client sends header "x-aster-subject: alice"
    And the client requests "/api/notebooks"
    Then the response is 403

  Scenario Outline: Provider groups decide the aster role
    Given an ID token whose groups claim is "<groups>"
    Then the caller is granted role "<role>"

    Examples:
      | groups                          | role   |
      |                                 | viewer |
      | aster-editors                   | editor |
      | aster-editors,aster-admins      | admin  |
      | some-other-group                | viewer |
