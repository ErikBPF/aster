@contract
Feature: Notebooks serialize as a diff-friendly text file

  A notebook is one text file in git: a header line, a title, then one block per
  cell. The scenarios below are executed by crates/core/tests/features.rs.

  Scenario: Cells survive a round trip
    Given a notebook with the title "Sales"
    And a cell "c1" holding "SELECT 1"
    And a cell "c2" holding "SELECT 2" bound to engine "trino-local"
    When the notebook is written to text and read back
    Then the notebook has 2 cells
    And cell "c2" is bound to engine "trino-local"

  Scenario: A file without the aster header is rejected
    Given the text "# title: Sales\nSELECT 1\n"
    When the text is parsed as a notebook
    Then parsing fails as invalid
