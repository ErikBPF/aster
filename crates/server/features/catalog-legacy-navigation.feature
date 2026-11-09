@contract @unautomated
# Covered by polaris_generic_browse::legacy_namespace_can_open_catalog_details;
# this Gherkin scenario has no step binding yet.
Feature: Legacy catalog namespace navigation

  Scenario: Legacy namespace names remain navigable
    Given a visible catalog returns schema "sales" using its legacy namespace name
    When Alice selects schema "sales" in the catalog workspace
    Then the detail request succeeds and names schema "sales"
