@contract @unautomated
Feature: Cell conversation

  A cell carries its own small conversation about that cell's SQL. The panel is
  closed by default and, when open, the cell body splits with the editor at
  about 70 percent and the cell conversation at about 30 percent. It is scoped
  to that cell only: the cell SQL, the catalog and data-contract references for
  the tables the SQL names, and the last run result or error of that cell. It
  never reads the notebook conversation and never reads another cell's
  conversation or output.

  Scenario: The cell AI action opens an inline conversation panel
    Given a notebook with a cell "q1" whose SQL is "SELECT orderkey FROM orders"
    When Alice presses the AI action on cell "q1"
    Then the cell body shows the editor and the cell conversation side by side
    And the cell conversation occupies at most 30 percent of the cell width
    When Alice presses the AI action again
    Then the cell conversation is hidden

  Scenario: The cell conversation is grounded in the semantic layer
    Given a loaded data contract and a catalog table named "orders"
    When Alice asks the cell conversation a question naming orders
    Then the helper receives the SQL of cell "q1"
    And the helper receives the contract's semantics, the catalog schema and the semantic model for orders
    And the helper receives the last run result of cell "q1" when one exists

  Scenario: The cell conversation cannot read other conversations
    Given a notebook conversation whose saved history contains "quarterly revenue"
    And another cell "q2" whose conversation contains "sandwich recipe"
    When Alice asks the conversation of cell "q1" anything
    Then the helper request for cell "q1" contains neither "quarterly revenue" nor "sandwich recipe"
    And the saved history of cell "q1" is separate from the notebook conversation and from cell "q2"

  Scenario: An oversized cell prompt is refused
    Given a cell conversation prompt longer than 8192 characters
    When Alice sends it
    Then the server refuses it and no exchange is saved
