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

  Scenario: Single-click SQL insertion between cells
    Given a notebook with two SQL cells
    When Alice presses the Cell control below it
    Then a new SQL cell follows exactly that cell
    When Alice starts it with the percent-md marker and leaves the editor
    Then that cell renders Markdown
    And the notebook toolbar has no duplicate creation control

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

  Scenario: Deleting a session preserves the notebook
    Given Alice has multiple private notebook or cell sessions
    When Alice confirms deleting a selected session
    Then that exact session and its messages are removed
    And another session is selected when available
    And deleting the last session leaves an empty draft until Alice sends or starts a session
    And cancelling deletion leaves history and the current draft unchanged
    And stale revisions and unresolved tool turns refuse deletion
    And another subject or workspace cannot delete the session
    And restarting storage does not restore deleted legacy history

  Scenario: Notebook data context is disclosed without changing it
    Given a notebook with a selected catalog and schema
    When Alice opens Data and searches catalogs or schemas
    Then the selection stays unchanged until Alice selects an option
    And full namespace paths remain visible
    When Alice explicitly changes the catalog
    Then the schema clears with visible feedback

  Scenario: Compute and model controls retain their scopes
    Given Alice opens a notebook
    Then Compute shows its selected target and reported health
    And an unresolved default is labeled explicitly
    And the assistant header offers the notebook-wide AI model choice
    And opening these controls does not run queries

  Scenario: Secondary cell actions are in an accessible menu
    Given Alice opens a cell's actions
    Then its identifier and delete are available
    And move up and move down remain visible without opening the menu
    And Run and AI remain directly available for SQL cells

  Scenario: Legacy mixed cell contexts require an explicit notebook choice
    Given SQL cells have different legacy catalog or schema values
    When Alice edits SQL without selecting notebook context
    Then the legacy cell metadata remains intact
    And running asks for an explicit notebook data context
    When Alice selects a notebook catalog and schema
    Then all SQL cells use that saved pair
    And unrelated cell metadata remains intact

  Scenario: Models remain directly manageable from notebooks
    Given Alice opens a notebook
    When Alice follows Manage models
    Then the Models tab opens with registration controls governed by her permissions

  Scenario: Saved Markdown becomes runnable SQL when its marker is removed
    Given Alice reopens a saved Markdown cell
    When Alice edits it to SQL without the percent-md marker
    And Alice leaves the editor
    Then the SQL Run button is visible
    And Run all can reach the cell without a browser error


  Scenario: Cell operations are recoverable
    Given Alice selected two cells with source and metadata
    When Alice duplicates, moves or deletes the selected cells
    Then Undo restores their previous order and identities
    And Redo reapplies the operation without duplicating assistant sessions

  Scenario: Notebook commands keep secondary controls out of the toolbar
    When Alice opens Notebook commands using its button or keyboard shortcut
    Then Alice can search for editing, outline, source search and revision actions
    And closing the dialog restores keyboard focus

  Scenario: Source replacement requires a current preview
    Given Alice previews a literal replacement in notebook source
    When the source changes before she applies it
    Then replacement is refused until she previews again
    And an applied replacement can be undone

  Scenario: Export describes only the displayed result
    Given a query result is truncated
    When Alice downloads displayed rows as CSV
    Then the download indicates truncation and escapes quotes and newlines
    And formula-shaped text is protected from spreadsheet execution
    And clearing displayed output does not delete stored exchanges

  Scenario: History restores through an explicit conflict boundary
    Given Alice previews a previous authorized notebook revision
    When another edit changes the current notebook before restore
    Then restore is refused without refreshing or retrying its precondition
    And historical metadata and notebook context remain available in the preview


  Scenario: Selection exposes contextual actions
    Given Alice selects two cells with the mouse or keyboard
    Then a selection toolbar displays the selected count
    And Run, move, duplicate, delete and clear actions target that selection
    And per-cell buttons continue to target their own cell

  Scenario: Keyboard cell navigation stays separate from text editing
    Given Alice is editing a cell with autocomplete closed
    When she presses Escape
    Then the cell has navigation focus
    And arrows select cells and Shift arrows extend the selection
    When she presses Enter
    Then only the active cell is selected and its editor receives focus
    And typing letters in editors, assistant inputs or dialogs never inserts or deletes cells

  Scenario: Keyboard execution and insertion use the active selection
    Given Alice has a selected range in navigation mode
    When she presses Control Enter
    Then selected SQL cells run in document order through the existing queue
    When she presses Shift Enter
    Then the range runs and focus advances beyond it
    And A and B insert SQL cells above and below only in navigation mode

  Scenario: Shortcut discovery does not require a command button
    Then notebook overflow exposes Find action
    And shortcut help is reachable by button and question mark in navigation
    And repeated action keys do not create repeated mutations

  Scenario: Assistant queries transfer independently without execution
    Given a reply contains multiple fenced SQL blocks
    When I send one query to an explicit cell destination
    Then only that query replaces the destination SQL with Undo available
    And no query executes before I choose Run

  Scenario: Assistant transfer preserves changed or missing destinations
    Given a reply offers a destination cell
    When that cell is deleted, moved or changes type
    Then transfer refuses a deleted target and requires review of a moved or changed target
    And a Markdown destination keeps its content beside a new SQL cell
