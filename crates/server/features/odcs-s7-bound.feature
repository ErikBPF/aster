@contract @odcs-s7-bound
Feature: Contract context supports manual query review without execution
  Scenario: Authorized contract browsing survives denied observations
    Then S7 browser preserves admitted meaning and clears revoked context

  Scenario: Manual query building retains semantic context for review
    Then S7 browser reviews edited drafts with selected meaning and binding gaps
