@contract @unautomated
# Draft Gherkin; executable coverage: catalog_assets::tests.
Feature: Catalog images respect entity and checkout boundaries

  Scenario: Table images remain isolated from sibling tables
    Given two visible tables have separate image directories
    When Alice lists images for one table
    Then only that table's images are listed
    And sibling and legacy shared images cannot be read through that table

  Scenario: An image directory cannot expose files outside the checkout
    Given a table image directory links outside the configured checkout
    When Alice lists that table's images
    Then the request is refused without exposing image names

  Scenario: Table images do not alias nested schema images
    Given a schema namespace contains the segments sales, tables and orders
    And table orders belongs to schema sales
    Then the table and schema image directories are distinct
