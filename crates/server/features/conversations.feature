@contract @unautomated
# Examples are covered by router, PostgreSQL and browser checks; no Gherkin steps yet.
Feature: Private notebook helper conversations
  Conversation history belongs to the authenticated user and notebook.
  The sidebar holds replies until the user explicitly inserts or runs SQL.

  Scenario: Follow-up questions survive reload
    Given Alice has exchanged a question and answer in notebook sales
    When Alice sends a follow-up question
    Then the helper receives the saved user and assistant messages
    And reloading sales shows both completed exchanges

  Scenario: Conversations are isolated
    Given Alice has a conversation in sales
    When Bob opens sales or Alice opens another notebook
    Then that conversation's messages are not returned

  Scenario: A failed reply does not corrupt history
    Given Alice has a completed exchange
    When the helper fails or conversation storage refuses the write
    Then no incomplete exchange is saved
    And the composer retains the unsent draft

  Scenario: Overlapping sends do not overwrite history
    Given two clients have the same saved conversation revision
    When both send a message
    Then at most one exchange commits at that revision
    And the other receives a conflict and reloads saved history
    And resubmission requires an explicit user action

  Scenario: Conversation storage can be delegated
    Given metadata and conversation storage use different PostgreSQL databases
    When Alice completes an exchange and the server reconnects
    Then the exchange is retained only in the conversation database
    And unrelated metadata tables are absent from that database

  Scenario: The sidebar is expandable and replies are inert
    When Alice opens, expands and collapses the notebook chat sidebar
    Then she can read history and send a question using the keyboard
    And neither SQL nor model-supplied HTML executes automatically
    And cell SQL changes only after an explicit insertion action

  Scenario: Conversation affinity belongs to the server
    Given Alice opens an existing conversation
    When her request includes a forged upstream session header
    Then the helper receives the persisted conversation identifier

  Scenario: History has explicit limits
    When a prompt exceeds 8 KiB or a reply exceeds 32 KiB
    Then the turn is refused without changing saved history
    And a transcript exceeding 200 messages or 256 KiB is refused without truncation

  Scenario: Reference material is retrieval-scoped
    Given a loaded data contract and a catalog table named "orders"
    When Alice asks a question naming orders
    Then the helper receives the contract's semantics, the catalog schema and the semantic model for orders
    And an unrelated question is sent without that reference material
