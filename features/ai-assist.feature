@contract @unautomated
# Contract-only: the equivalent Rust tests plus the server smoke path execute
# these behaviors today; binding them to a Gherkin runner is tracked in the
# proposal under the S11/S10 follow-up work.
Feature: User-registered LLM endpoints
  Users bring their own OpenAI-compatible endpoint, model and token. The token is
  held server-side and every completion is proxied, so no client ever sees it.

  Background:
    Given the server is running
    And subject "alice" is signed in with role "editor"

  Scenario: Registering an endpoint
    When alice posts a base url, a model and a token to /api/llm
    Then the response reports the base url and model
    And the stored configuration belongs to alice

  Scenario: The token is never returned
    Given alice has registered an endpoint with token "secret-token"
    When alice reads /api/llm
    Then the response names the base url and model
    And the response contains no token

  Scenario: Generating SQL reuses the registered endpoint
    Given alice has registered a working endpoint
    When alice posts a prompt to /api/ai
    Then the server calls the endpoint's chat completions path with the stored token
    And the response carries the model's SQL as "sql"

  Scenario: Generation without an endpoint
    Given alice has registered no endpoint
    When alice posts a prompt to /api/ai
    Then the request is refused with 404 and mentions the missing endpoint

  Scenario: Viewers cannot spend a token
    Given subject "bob" is signed in with role "viewer"
    And bob has registered an endpoint
    When bob posts a prompt to /api/ai
    Then the request is refused with 403
