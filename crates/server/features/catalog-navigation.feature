@contract
Feature: Navigable catalog workspace

  The catalog page is a lateral tree over the same authorized inventory. Each
  selected entity has details, Git-backed docs, history and effective
  permissions. Docs are Markdown and every save is a local Git commit.

  Background:
    Given the server is running with engine "trino-local" registered
    And subject "alice" has a visible catalog "polaris-local" with schema "default" and table "orders"

  Scenario: The lateral tree expands catalogs, schemas and tables
    When Alice opens the catalog tree
    Then the tree lists catalog "polaris-local"
    When Alice expands catalog "polaris-local"
    Then the tree lists schema "default"
    When Alice expands schema "default"
    Then the tree lists table "orders"

  Scenario: Search index contains nested tables
    When Alice opens the catalog search index
    Then the search index includes table "orders"

  Scenario: A table detail shows metadata and effective permissions
    When Alice selects catalog "polaris-local" schema "default" table "orders"
    Then the detail names table "orders"
    And the detail lists the observed columns
    And the permissions name bound engine "trino-local" as granted
    And the permissions do not expose a raw binding secret

  Scenario: A catalog document is edited and versioned
    Given the catalog asset repository is empty
    When Alice saves catalog docs for table "orders" with "# Orders"
    Then the document has a revision
    And the history contains a commit by "alice"
    When Alice saves catalog docs for table "orders" with "# Orders v2"
    Then the document has a new revision
    When Alice saves catalog docs for table "orders" with the first revision
    Then the save is refused as a conflict
    When Alice reads the first revision
    Then the returned Markdown is "# Orders"

  Scenario: Hidden entities are refused by detail, docs and assets
    Given the catalog asset repository is empty
    And Alice expands catalog "polaris-local"
    And the table asset directory contains "diagram.svg" and "notes.txt"
    And the catalog is hidden from metadata
    When Alice requests the hidden catalog detail
    Then the call fails as permission denied
    When Alice tries to save hidden catalog docs for table "orders" with "# hidden"
    Then the call fails as permission denied
    When Alice reads hidden catalog docs for table "orders"
    Then the call fails as permission denied
    When Alice requests hidden asset "diagram.svg"
    Then the request is refused with permission denied

  Scenario: Only images are served as table assets
    Given the table asset directory contains "diagram.svg" and "notes.txt"
    When Alice requests asset "diagram.svg"
    Then the response is an image
    When Alice requests asset "notes.txt"
    Then the request is refused
