@contract @unautomated
Feature: Persisted notebook data context
  Draft scenarios; executable checks are core notebook unit tests and server notebook ownership tests.

  Scenario: Context survives storage and older clients
    Given a notebook selects warehouse and sales.eu
    When it is saved and reopened through Git, REST or RPC
    Then its notebook context remains warehouse and sales.eu
    And an older client omitting context preserves that saved pair
    And explicit empty context clears the pair

  Scenario: New context is not silently erased by an older binary
    Given a notebook has explicit notebook context
    When it is serialized
    Then the file uses notebook format v3 with a JSON context header
    And the current reader still accepts v1 and v2 notebooks

  Scenario: Notebook query scope is authoritative
    Given a notebook selects warehouse and sales.eu
    When a notebook query omits catalog and schema
    Then the saved pair reaches query execution and audit
    And conflicting request context is refused before execution
    And an explicitly empty notebook catalog refuses execution
