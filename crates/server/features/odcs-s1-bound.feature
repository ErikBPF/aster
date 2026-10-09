@contract @odcs-s1-bound
# Executes the same HTTPS/router assertions as ai_context_boundary.
Feature: Metadata and notebook context respect admitted scope
  Scenario: Denied metadata stays off the wire
    Then protected metadata makes zero catalog or helper requests with a separate allowed schema control

  Scenario: Mixed unbound catalogs are never retrieved
    Then mixed metadata sends the allowed schema but never reads unbound catalogs or discloses unadmitted contracts

  Scenario: Notebook index belongs to the admitted workspace
    Then two teams and two sessions and a personal workspace send only their admitted notebook index
