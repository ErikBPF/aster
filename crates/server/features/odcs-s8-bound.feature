@contract @odcs-s8-bound
Feature: Owner-scoped physical inventory in compiled catalog mode
  Scenario: Physical entries require current owner or contract admission
    Then S8 physical inventory scopes entries and refuses unknown evidence

  Scenario: Unselected AI cannot bypass physical inventory admission
    Then S8 unselected assistance discloses no physical metadata

  Scenario: Unauthorized schema probes do not reveal configured existence
    Then S8 unknown and unauthorized schemas have indistinguishable responses

  Scenario: Inventory decisions share one fresh identity snapshot
    Then S8 ownership and contract admission use one current identity

  Scenario: Nested annotations contribute to the declared semantic facet
    Then S8 nested properties items and maps retain declared semantic annotations

  Scenario: A mock materialization grant does not disclose its sibling table
    Then mock table contracts keep sibling artifacts and inventory independently admitted
