@contract @unautomated
# Browser-only catalog workspace behavior. The server contract is
# `catalog-navigation.feature`.
Feature: Catalog navigation workspace

  Scenario: The lateral tree drives the detail pane
    Given visible catalogs and schemas
    When Alice opens the catalog page
    Then the left rail lists every visible catalog
    When Alice expands a catalog and a schema
    Then the rail lists their schemas and tables
    When Alice selects a table
    Then the detail pane shows Details, Docs, History and Permissions

  Scenario: Details and permissions stay honest
    When Alice opens a table
    Then the Details tab shows format, location and observed columns
    And the Permissions tab names bound engines and grant state
    And no raw binding credential or secret appears

  Scenario: Markdown docs are editable and versioned
    Given the catalog asset checkout is configured
    When Alice edits the table document and saves
    Then the preview renders sanitized Markdown and Mermaid
    And a Git commit is created with Alice as author
    And the History tab lists that commit
    When Alice opens an older revision
    Then the document is read-only at that revision

  Scenario: Images are the only assets
    Given a table asset directory with PNG and text files
    Then the PNG renders in the image gallery
    And the text file is not offered

  Scenario: Search and menu controls
    When Alice types "orders" in the catalog search
    Then matching tables are listed and selecting one opens it
    When Alice presses the menu button
    Then the catalog rail closes and the detail pane expands
    When Alice presses it again
    Then the rail returns in its previous state

  Scenario: Deep links restore the selected entity
    When Alice opens a catalog table URL with catalog, namespace and table
    Then the tree expands to that table and the detail pane selects it
