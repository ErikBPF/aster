@contract @odcs-s2-bound
Feature: Internal compiled ODCS intake and lossless identity
  Scenario: Compiled intake needs no author files
    Then compiled artifact preservation and manifest selection pass without author files
  Scenario: A multi-object contract retains its identity
    Then all compiled document objects and original identity remain intact
  Scenario: Invalid v3.2 does not become legacy
    Then invalid v3.2 fails the pinned offline schema
  Scenario: Schema-valid v3.1 is rejected by the support gate
    Then schema-valid v3.1 fails support without conversion
  Scenario: Namespace segments survive adapter and API boundaries
    Then qualified namespaces retain exact provider segments through API transport
