@contract @unautomated
# Planning contract only; no executable Gherkin bindings or RED evidence yet.
Feature: Admin-managed remote MCP connections

  Scenario: Only administrators register MCP servers
    Given Alice is an ordinary signed-in user
    When Alice registers a remote MCP connection
    Then registration is refused
    And no connection is persisted

  Scenario: A team grant controls tool discovery
    Given an administrator registered a remote MCP server
    And granted its use to team "analytics"
    When a current member of team "analytics" opens available tools
    Then the connection is available
    And users without a grant cannot discover its tools

  Scenario: Revocation prevents the next external tool call
    Given Alice selected an authorized MCP connection
    When an administrator revokes Alice's access
    Then Alice's next tool call is refused before contacting the server

  Scenario: A direct user grant controls tool discovery
    Given an administrator registered a remote MCP server
    And granted its use directly to Alice
    When Alice opens available tools
    Then the connection is available
    And Bob without a grant cannot discover its tools

  Scenario: Skill text cannot grant MCP access
    Given Alice can read a skill that instructs her model to use a restricted tool
    And Alice has no grant for that tool's MCP connection
    When the model requests the restricted tool
    Then the call is refused before contacting the server
    And no bundled skill code is executed

  Scenario: Admin-approved read tools run without another confirmation
    Given an administrator approved the current definition of a tool as read-only
    And Alice has access to its enabled connection
    When Alice runs the tool with valid arguments
    Then the server executes the read without requesting confirmation

  Scenario: Changing actions require an exact single-use confirmation
    Given Alice has access to a tool not approved as read-only
    When Alice requests the tool with arguments
    Then no tool call is sent before she confirms
    And the confirmation identifies the connection, tool and exact arguments
    And changing the arguments, user or connection revision invalidates confirmation
    And a consumed or expired confirmation cannot execute again

  Scenario: Donsetch provides a disposable demonstration
    Given isolated development mode explicitly enables the Donsetch mock endpoint
    When Alice confirms its simulated note action
    Then the result identifies itself as a mock receipt
    And no external system is changed
