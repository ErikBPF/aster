# Draft behavior contract: runnable router/adapter/browser checks provide coverage;
# this file is not yet bound to Gherkin steps and is not itself an automated test.
Feature: Persistent notebooks over two real benchmark catalogs
  Background:
    Given the isolated demo uses owner-checked local Git notebooks
    And real Trino benchmark catalogs "tpch" and "tpcds" at tiny scale
    And contract membership policy remains enabled

  Scenario: Login leads to a usable persistent notebook
    When Alice completes a fresh browser login
    Then the landing page offers notebook creation
    When Alice creates a notebook, edits SQL, saves and reloads
    Then the saved SQL is preserved
    And another user cannot read or overwrite Alice's notebook

  Scenario Outline: A notebook executes against its selected catalog
    Given Alice has selected catalog "<catalog>"
    When Alice runs "<sql>" in a saved notebook cell
    Then the real backend returns the expected nonempty result
    And the displayed catalog context is "<catalog>"
    Examples:
      | catalog | sql                                      |
      | tpch    | SELECT count(*) FROM tpch.tiny.nation     |
      | tpcds   | SELECT count(*) FROM tpcds.tiny.call_center |

  Scenario: Catalog inventory and contracts cover all benchmark tables
    When Alice browses each benchmark catalog
    Then its table inventory matches the real backend metadata
    And each table has its own source-pinned synthetic ODCS contract
    And structured contract details display all populated fields
    And original source and provenance remain available
    And declared claims remain distinct from observations and quality results

  Scenario: Demo access checks survive local notebook activation
    When a viewer attempts notebook writes or query execution
    Then the operation is denied
    When Alice's contract membership is revoked
    Then previously visible contract details become unavailable
    And stale rendered contract details are cleared on access revalidation
