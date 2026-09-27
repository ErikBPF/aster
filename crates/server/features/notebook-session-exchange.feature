@contract
Feature: Notebook session exchange

  The notebook sidebar conversation discusses the whole notebook. It reaches
  cell material only through five explicit exchange operations, each scoped to
  the notebook and the requesting principal: FetchResult (one cell's last
  result), FetchQuery (one cell's SQL), FetchSummary (the notebook summary and
  the cell index), SendSummary (replace the notebook summary) and UpdateQuery
  (replace one cell's SQL). Every operation stays inside the one notebook and
  none of them reads any cell conversation.

  Scenario: The session reads one cell's query and result
    Given a notebook "sales" with a cell "q1" whose SQL is "SELECT 1 AS one" and whose last result column is "one"
    When the notebook session fetches the query and the result for cell "q1"
    Then it receives the SQL "SELECT 1 AS one" and the result column "one"

  Scenario: The session replaces a cell query
    Given a notebook "sales" with a cell "q1" whose SQL is "SELECT 1 AS one"
    When the notebook session updates the query of cell "q1" to "SELECT 2 AS two"
    Then cell "q1" holds "SELECT 2 AS two" at a higher revision
    And an update carrying the previous revision is refused as a conflict

  Scenario: The session round-trips the notebook summary
    Given a notebook "sales" with no summary
    When the notebook session sends the summary "Orders experiments: revenue by region"
    Then a later fetch of the summary returns "Orders experiments: revenue by region"

  Scenario: The exchange carries no conversation transcript
    Given a notebook "sales" with a cell "q1" whose SQL is "SELECT 1 AS one" and whose last result column is "one"
    When the notebook session fetches the summary, the query and the result for cell "q1"
    Then what it receives is the summary, the query and the result
    And none of what it receives carries a conversation transcript

  Scenario: Each notebook keeps its own cell material
    Given a notebook "sales" and a notebook "ops" that each have a cell "q1"
    When a session for "sales" fetches the result for cell "q1"
    Then it receives the result held by "sales"
    And a fetch for "ops" returns the result held by "ops"
