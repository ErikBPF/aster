@contract @unautomated @odcs-ai-catalog
# R20 target: physical inventory, owner-only uncovered metadata, contracted access.
# Docs/mock fixtures only; runtime policy is not implemented or bound.
# No runnable step bindings and no observed RED/GREEN. This file is not an automated test.
# Plan: docs/plans/odcs-ai-catalog.md; source base eb397d8; revision r20.
# Q1 compatibility accepted: v3.2 only; conversion upstream outside Aster.
# R9 settles explicit manifest pins, fail-closed bundle intake and versioned Aster bindings.
# Q7 model accepted: whole-contract grants to existing Aster teams, default-deny.
# Prior S3 implementation supplies grants, not physical/contract query admission.
# Q8 outcome accepted: semantic context materially informs query building/review.
# Historical Q5 budgets are settled; Q11 authoritative query-target proof remains.
Feature: Unified catalog inventory with contract and semantic context
  Inventory entries expose independent physical, contract and semantic evidence.
  Whole-contract existing-team grants and separate engine authority remain required.
  Only schema owners see uncovered metadata; all data access requires a contract.
  Semantics never grants access, and no admin bypass is assumed.

  Scenario: A contract alone does not make absent data accessible
    Given an admitted contract is bound to an object known absent from its catalog
    When its owner or an administrator requests data access or sharing
    Then the data is not accessible or shareable

  Scenario: Only the schema owner group sees uncovered metadata
    Given a physically cataloged table has no contract and its schema has an owner group
    When an owner and a nonmember request discovery and metadata detail
    Then only the owner sees the uncovered table metadata
    And neither may access or share the table data without a contract

  Scenario: Semantic description artifacts do not imply runtime support
    Given a schema offers a semantics.yaml with explicit contract object associations
    When its semantics have not been admitted by an implemented input path
    Then the product does not advertise those definitions as loaded or executable

  Scenario: One entry has independent physical contract and semantic facets
    Given an authorized inventory entry has verified physical catalog evidence
    And the entry has an explicitly associated contract admitted to the caller's team
    And that contract declares semantic meaning for only one of its fields
    When the caller inspects the entry in the unified catalog view
    Then physical presence and contract coverage are shown together
    And semantic coverage identifies the declared field without claiming complete coverage
    And those facets do not claim permission to execute a query

  Scenario: Unavailable observation is not evidence of physical absence
    Given the caller has a whole-contract team grant for a declared object
    And physical observation is unavailable
    When the caller inspects the separately authorized contract artifact
    Then the authorized contract meaning remains available
    And physical presence is unknown rather than absent
    And the unavailable observation does not authorize data access or sharing
    And the artifact alone does not create an accessible physical inventory entry
    And no missing semantic definitions or physical identifiers are invented

  # S5's three scenarios now live in odcs-s5-bound.feature, backed by HTTPS captures.

  # Original-source portion is bound in odcs-s4-original-preview-bound.feature.
  # Whole S4, including derived Cube/new-document behavior, remains pending Q4.
  Scenario: ODCS preview is independently valid
    Given a supported valid multi-object ODCS v3.2 document has contract version "1.4.0"
    When a caller authorized for the full document requests its ODCS preview endpoint
    Then the preview validates offline against the pinned official schema
    And its document and object identities and contract version are preserved
    And all objects, servers, nested properties, semantics and standard sections remain intact
    And the full parsed preview equals the original document when unchanged
    And selecting an object for derived Cube output does not truncate the ODCS preview
    And physical data types are not emitted as invalid logical types

  Scenario: Semantic preview never guesses support
    Given a preserved ODCS document has semantics whose translation is not established
    When an authorized caller requests a derived preview
    Then Aster does not claim a supported translation by guessing an aggregation
    And the authoritative ODCS document remains available and unchanged

  # S6's current-schema/pagination scenario is bound in odcs-s6-bound.feature.

  Scenario: Catalog comparison and preview have no write side effects
    Given an authorized caller opens a bound object's catalog comparison
    When the caller requests a supported semantic preview
    Then the view distinguishes declared meaning from observed structure
    And the response is reviewable text with provenance
    And no contract or model is written or published
    And no SQL is executed
