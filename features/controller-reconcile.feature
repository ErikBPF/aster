@contract @unautomated
Feature: Controller reconcile loop

  The controller mirrors the configured pool into the metadata database and
  prunes expired audit events. It owns no user-facing traffic.

  Background:
    Given the controller runs with DATABASE_URL set

  Scenario: Configured engines are mirrored with a fresh health observation
    Given engine "trino-local" is configured and unreachable
    When the controller completes a reconcile pass
    Then the "engines" table holds "trino-local" with health "unavailable"
    And the row holds the endpoint and routing group from configuration

  Scenario: Engine health is refreshed on the next pass
    Given the "engines" row for "trino-local" holds health "unavailable"
    And the engine becomes reachable
    When the controller completes another reconcile pass
    Then the row for "trino-local" holds health "healthy"

  Scenario: Configured catalogs are mirrored
    When the controller completes a reconcile pass
    Then the "catalogs" table holds the configured catalog with its kind and health

  Scenario: Expired audit events are pruned
    Given an audit event older than the retention window
    And an audit event inside the retention window
    When the controller completes a reconcile pass
    Then only the expired audit event is deleted

  Scenario: The controller refuses to start without a metadata database
    Given DATABASE_URL is unset
    When the controller starts
    Then it exits with an error naming DATABASE_URL

  Scenario: A failing pass does not stop the loop
    Given the metadata database is briefly unavailable
    When a reconcile pass fails
    Then the controller logs the failure and reconciles again on the next tick
