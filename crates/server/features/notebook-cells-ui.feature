@contract @unautomated
Feature: Notebook session tabs, insertion rows and Markdown cells

  Browser-only behavior recorded for human and eventual browser-runner
  validation. The server contract is `notebook-sessions.feature`.

  Scenario: Legacy per-cell engines survive autosave
    Given a notebook with no notebook engine and two cells pinned to different engines
    When Alice edits one cell and autosave runs
    Then the saved document keeps both per-cell engine markers
    And it does not add a notebook engine header

  Scenario: Edits autosave after a short idle
    Given a notebook with one cell
    When Alice edits the cell and stops typing
    Then the notebook saves without pressing Save
    And the status line reports the autosave

  Scenario: One add control below every cell
    Given a notebook with one SQL cell
    When Alice presses the Cell control below it
    Then one new SQL cell follows it
    And there is no separate Markdown creation control
    When Alice types the percent-md marker and leaves the cell
    Then that cell becomes Markdown

  Scenario: Add cell below all cells
    Given a notebook with several cells
    When Alice presses Add cell below the cell list
    Then a new SQL cell is appended after the last cell

  Scenario: Notebook session tabs show the latest session by default
    Given a notebook with two saved notebook sessions
    When Alice opens the notebook chat
    Then the newest session tab is active
    And every older tab shows a short phrase and a date
    And the new-session control is a tab, not a separate button

  Scenario: Alice starts and selects cell sessions
    Given a notebook with a cell q1
    When Alice opens the cell assistant and starts a new session
    Then a new q1 tab is active
    And selecting an older q1 tab loads its messages

  Scenario: A notebook reply can fill the current cell
    Given the notebook assistant replied with a fenced SQL block
    And Alice last focused cell q2
    When Alice presses Insert into current cell
    Then q2's editor holds the suggested SQL
    And the SQL has not run

  Scenario: Markdown cells render inertly with Mermaid
    Given a Markdown cell containing a heading and a mermaid fenced block
    When Alice leaves edit mode
    Then the heading renders as a heading
    And the Mermaid block renders as a diagram
    And no script, event handler or remote iframe is present

  Scenario: Fetch controls attach exchange material
    Given a notebook with cell q1 whose SQL and last result are recorded
    When Alice presses Fetch query and Fetch result in notebook chat
    Then the composer context contains q1's SQL and result rows
    And sending is still an explicit action

  Scenario: The notebook engine applies to every cell
    Given a notebook engine of trino-local
    When Alice opens the notebook
    Then the toolbar shows one engine selector
    And no cell shows its own engine selector
