@contract @unautomated
# Contract-only: the unit tests in crates/core/src/llm.rs and crates/server/src/ai.rs
# execute these behaviors today; binding them to a Gherkin runner is tracked in the
# proposal under the S10 follow-up work.
Feature: User-registered LLM helpers
  Users bring their own OpenAI-compatible endpoints. Several may be registered per
  subject, each under a short name, and a notebook picks one. The token is held
  server-side and every completion is proxied, so no client ever sees it.

  Background:
    Given the server is running
    And subject "alice" is signed in with role "editor"

  Scenario: Registering several helpers
    When alice puts "lab-qwen" to /api/llm/lab-qwen with a base url, a model and a token
    And alice puts "cloud" to /api/llm/cloud with a base url, a model and a token
    Then listing /api/llm reports "cloud" and "lab-qwen"
    And each entry names its base url and model

  Scenario: A helper name is a safe path segment
    When alice puts "../escape" to /api/llm
    Then the request is refused with 400

  Scenario: The token is never returned
    Given alice has registered "lab-qwen" with token "secret-token"
    When alice reads /api/llm
    Then the response names the base url and model
    And the response contains no token

  Scenario: A helper URL cannot carry credentials
    When alice puts a helper whose base URL contains a username and password
    Then the request is refused with 400
    And listing /api/llm does not contain that helper

  Scenario: Legacy URL credentials are not returned
    Given a previously saved helper URL contains a username and password
    When alice reads /api/llm
    Then neither credential appears in the response
    And generation refuses that helper before an upstream request

  Scenario: Another subject's helper of the same name is invisible
    Given alice has registered "lab-qwen"
    And subject "bob" is signed in with role "editor"
    When bob reads /api/llm
    Then the response lists no helpers

  Scenario: Removing a helper
    Given alice has registered "lab-qwen" and "cloud"
    When alice deletes /api/llm/lab-qwen
    Then listing /api/llm reports only "cloud"

  Scenario: Generating SQL with a named helper
    Given alice has registered "lab-qwen" and "cloud"
    When alice posts a prompt to /api/ai with helper "cloud"
    Then the server calls the cloud endpoint's chat completions path with its token
    And the response carries the model's SQL as "sql"

  Scenario: Two users can use overlapping names without sharing destinations
    Given alice registers "alpha" and "beta" at distinct endpoints
    And bob registers "alpha" at the beta endpoint with a different model and token
    When alice generates with "alpha" and "beta" and bob generates with "alpha"
    Then each request reaches only its subject's selected endpoint with its model and token
    And replacing or deleting alice's "alpha" does not change bob's "alpha"

  Scenario: Generating SQL without naming a helper uses the first registered one
    Given alice has registered only "lab-qwen"
    When alice posts a prompt to /api/ai
    Then the server calls the lab-qwen endpoint

  Scenario: Generating without any helper
    Given alice has registered no helper
    When alice posts a prompt to /api/ai
    Then the request is refused with 404 and mentions the missing endpoint

  Scenario: An unregistered helper name is refused
    Given alice has registered only "lab-qwen"
    When alice posts a prompt to /api/ai with helper "missing"
    Then the request is refused with 404 and names "missing"

  Scenario: An explicit empty helper never selects a default
    Given alice has registered "lab-qwen"
    When alice posts a prompt to /api/ai with an empty helper name
    Then the request is refused with 400 before contacting the helper

  Scenario: Viewers cannot spend a token
    Given subject "bob" is signed in with role "viewer"
    And bob has registered "lab-qwen"
    When bob posts a prompt to /api/ai
    Then the request is refused with 403

  Scenario: Helper requests identify the client conversation
    Given the notebook page has a random conversation identifier
    When alice generates SQL twice from that page
    Then both upstream requests carry the same x-opencode-session identifier
    And the upstream user agent identifies Aster
    And no browser cookies are forwarded

  Scenario: Legacy callers have independent standalone conversations
    When alice posts a prompt without a conversation identifier
    Then the upstream request receives a fresh random conversation identifier

  Scenario: Invalid conversation identifiers are refused
    When alice posts a prompt with a conversation identifier containing spaces
    Then the request is refused with 400 before contacting the helper
