# Contract for the secret provider port (crates/core/src/secrets.rs).
# Executed by crates/core/tests/features.rs against the in-memory provider; the
# environment provider has its own unit tests in that module.
@contract
Feature: Secret resolution
  A secret lives wherever the deployment put it — an ExternalSecret in
  Kubernetes, the shell for a local run. The process asks a provider for a key
  and never logs a value.

  Background:
    Given a secret store

  Scenario: A configured key resolves
    Given the secret store holds "DATABASE_URL" = "postgres://localhost/aster"
    When the server requires "DATABASE_URL"
    Then it receives "postgres://localhost/aster"

  Scenario: A key that was never set is refused by name
    When the server requires "ASTER_STATE_URL"
    Then it is refused naming "ASTER_STATE_URL"

  Scenario: An empty value counts as unset
    Given the secret store holds "ASTER_OIDC_CLIENT_SECRET" = ""
    When the server requires "ASTER_OIDC_CLIENT_SECRET"
    Then it is refused naming "ASTER_OIDC_CLIENT_SECRET"

  Scenario: An optional key is reported as absent, not as an error
    When the server looks up "DATABASE_URL"
    Then the lookup reports nothing configured
