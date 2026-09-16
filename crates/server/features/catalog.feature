@contract @unautomated
Feature: Catalog browsing

  The catalog tab navigates namespaces, tables and columns of a registered
  catalog. It reads metadata only; permissions are enforced by the engine.

  Background:
    Given the caller is signed in as an editor
    And catalog "polaris-local" is registered

  Scenario: The catalog page lists the registered catalogs and their namespaces
    When the browser requests "/catalog"
    Then the page names catalog "polaris-local"
    And each namespace is a link to "/catalog/polaris-local/<namespace>"

  Scenario: A namespace page lists its tables
    Given namespace "sales" holds tables "orders" and "customers"
    When the browser requests "/catalog/polaris-local/sales"
    Then the page links "/catalog/polaris-local/sales/orders"
    And the page links "/catalog/polaris-local/sales/customers"

  Scenario: A table page lists its columns
    Given table "sales.orders" has columns "id bigint" and "total decimal"
    When the browser requests "/catalog/polaris-local/sales/orders"
    Then the page lists column "id" and column "total"

  Scenario: An unreachable catalog reports its error instead of failing the page
    Given the catalog endpoint is unreachable
    When the browser requests "/catalog"
    Then the response is 200
    And the page shows the catalog error

  Scenario: Browsing requires a session
    Given no session cookie
    When the browser requests "/catalog"
    Then the response is a redirect to "/login"

  Scenario: An unknown catalog is a 404
    When the browser requests "/catalog/nope/sales"
    Then the response is 404
