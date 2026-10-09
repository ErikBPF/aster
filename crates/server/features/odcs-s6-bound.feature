@contract @odcs-s6-bound
# Bound to real isolated HTTPS adapter captures and server registration-to-wire checks.
# Assertion RED was observed before fixes; this binding was added afterwards.
# These fixtures do not authorize or prove live-provider activation (Q6).
Feature: Trustworthy bounded catalog observations
  # Review regressions bind both partial OAuth startup refusals (zero requests),
  # a shared OAuth/health deadline and cumulative paginated response budgets.
  Scenario: Current Iceberg schema and all pages are observed
    Then S6 catalog observations are current complete bounded and target authenticated
