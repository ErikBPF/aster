# Draft contract (@unautomated). The equivalent behavior is exercised by the
# unit tests in crates/catalogs/src/lib.rs (URL building, response parsing,
# health, token and client-credential handling) and by the catalog page smoke
# path in the justfile. Binding these steps to a Gherkin runner needs an Iceberg
# REST fixture (proposal IP S8/S11).
@contract @unautomated
Feature: Polaris catalog navigation

  aster reads namespaces, tables and columns through the Iceberg REST catalog
  API, so the catalog tab works against Polaris today and any other
  Iceberg-compatible catalog behind the same trait.

  Scenario: Namespaces are listed for a catalog
    Given a catalog endpoint that answers the namespace listing
    When the catalog lists namespaces
    Then the multi-level names are presented with their levels joined

  Scenario: Tables are listed for a namespace
    Given a namespace holding several tables
    When the catalog lists tables for that namespace
    Then each table is returned with its namespace

  Scenario: A table schema is read for the catalog tab
    Given a table registered with typed columns
    When the catalog reads that table's schema
    Then the columns are returned in order with their types

  Scenario: An unknown table is a not-found error
    Given a table that is not registered
    When the catalog reads that table's schema
    Then the caller sees a not-found error and the process stays up

  Scenario: An unreachable catalog degrades instead of failing
    Given a catalog endpoint that does not answer
    When the catalog health is checked
    Then the health is unavailable
    And a page rendered from it shows the error without a stack trace

  Scenario: A configured bearer token is attached to requests
    Given a catalog configured with an access token
    When the catalog calls the Iceberg REST API
    Then the request carries that token as a bearer credential

  Scenario: OAuth2 client credentials are exchanged for a token
    Given a catalog configured with a client id and secret but no token
    When the catalog calls the Iceberg REST API
    Then the credentials are exchanged for an access token
    And the request carries that token as a bearer credential
