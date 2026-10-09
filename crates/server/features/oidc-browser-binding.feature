# Draft Gherkin (no step runner). Executable router coverage is provided by
# the oidc_callback_* tests in tests/ai_registration.rs, observed RED then GREEN.
Feature: Browser-bound OIDC login
  Scenario: A callback cannot redeem another browser's pending login
    Given an initiating browser has a short-lived HttpOnly SameSite Lax binding cookie
    When a callback arrives without that binding or with another browser's binding
    Then no handshake is consumed and no provider exchange occurs
    And the initiating browser can still complete the original login exactly once

  Scenario: Matching callbacks terminate their browser binding
    Given a callback matches the initiating browser binding
    When the handshake expires or the provider refuses or completes the exchange
    Then the binding cookie is cleared with matching scope and security attributes
    And replay cannot mint a session

  Scenario: Forwarded headers cannot change cookie transport protection
    Given the trusted callback URI is configured for HTTPS or an explicit HTTP demo
    When a request spoofs the opposite Forwarded or X-Forwarded-Proto scheme
    Then cookie security follows the trusted callback configuration
    And HTTPS binding cookies use the host prefix with no Domain attribute
