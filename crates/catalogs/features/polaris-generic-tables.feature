# Draft behavior contract from the 2026-09-22 user clarification.
# Unbound: planned adapter HTTP fixtures and pinned Polaris/Delta integration runner.
@contract @unautomated
Feature: Polaris generic-table validation for Delta
  Generic table registration is distinct from Iceberg table metadata and from
  actual Delta data access. The evaluation reports each capability separately.

  Scenario: Discover a registered Delta table through the generic-table API
    Given a Polaris namespace contains a generic table named "orders_delta"
    And its registered format is "delta" and its location contains a Delta fixture
    When Aster lists and loads that table
    Then the table is identified as Delta
    And its registered location is retained without being treated as Iceberg metadata

  Scenario: Prove registered Delta data can be read by a connected compatible engine
    Given a Delta fixture is registered as a Polaris generic table
    And a connected engine has a verified compatible Delta client and data access
    When the engine resolves the registered table and reads its data
    Then the returned rows equal the independently defined fixture
    And the evaluation records the engine and client versions used

  Scenario: Unsupported format capability is visible
    Given a catalog-connected engine has no validated Delta support
    When the caller selects a registered Delta table
    Then Aster does not advertise that engine as able to query the table
    And a direct unsupported execution request is refused

  Scenario: Registration success is not reported as data-query success
    Given generic-table registration and lookup succeed
    And the compatible Delta engine cannot read the fixture
    When the evaluation result is recorded
    Then registration is reported separately from the failed data-query check
    And the evaluation does not report end-to-end Delta support
