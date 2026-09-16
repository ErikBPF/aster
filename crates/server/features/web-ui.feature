@contract @unautomated
# Behavior contract for the server-rendered web shell (S7).
#
# Status: contract-only. The pages, the escaping unit test and the live smoke
# run on `cache-host` execute these behaviors; binding them to a Gherkin runner is
# tracked in the proposal (IP S7+).

Feature: Web shell for notebooks and cells

  Background:
    Given the server is running with a notebook "sales" containing one cell
    And a signed-in subject with the editor role

  Scenario: Notebook list is rendered server-side
    When the subject requests the index page
    Then the response is 200
    And the response body links to "/notebooks/sales"

  Scenario: Notebook page renders its cells and embeds the notebook as JSON
    When the subject opens the "sales" notebook page
    Then the response is 200
    And the page contains a textarea with the cell SQL
    And the notebook JSON is embedded for the client script

  Scenario: Notebook text cannot break out of the page
    Given a notebook titled "<script>alert(1)</script>"
    When the subject opens that notebook page
    Then the title appears escaped in the markup
    And no script tag from the notebook text is present

  Scenario: An unidentified caller cannot read pages
    When a caller without identity requests the index page
    Then the response is 403

  Scenario: A viewer can read but the run button still requires the query grant
    Given a signed-in subject with the viewer role
    When the subject requests the index page
    Then the response is 200
    And posting a query as that subject is rejected 403

  Scenario: Saving from the page commits the edited cells
    When the subject saves the notebook with an edited cell
    Then the response is 200 with a revision
    And reading the notebook back returns the edited SQL
