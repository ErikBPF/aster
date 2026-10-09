@contract @odcs-s5-bound
# Bound to actual router calls and certificate-verified HTTPS helper captures.
# Q5 refusal/omission/replay policies explicitly accepted 2026-10-01.
Feature: Selected contract semantics in bounded query assistance
  Scenario: Reference context remains scoped and transient
    Then S5 selected reference is bounded and revoked history cannot replay

  Scenario: Denied observations stay out of authorized contract context
    Then S5 authorized meaning survives denied and unavailable observations

  Scenario: Query assistance uses semantic context in reviewable drafting
    Then S5 actual helper requests and returned drafts follow exact selected semantics
