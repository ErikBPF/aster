@contract @odcs-s4-original-preview-bound
# Narrow accepted original-source preview only. Whole S4 and Q4 remain pending.
Feature: Whole-document original ODCS preview
  Scenario: Original ODCS preview preserves all content without observation or execution
    Then S4 original preview preserves source bytes and schema-valid content under whole-document admission
