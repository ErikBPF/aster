# Behavior contract: SQL completion for the notebook editor.
# Status: bound to `crates/server/tests/contracts.rs`, which drives the router in
# process against a stub catalog. The browser side (the dropdown) is exercised
# manually; these scenarios pin what the server offers it.
@contract
Feature: SQL completion
  The editor asks the server what can follow the token under the caret, so a
  caller does not have to remember whether a name needs a catalog qualifier:
  `sales.part` fails on Trino until it is `polaris.sales.part`. The stub catalog
  in these scenarios is named `polaris-local` on purpose, to keep the path that
  quotes a non-identifier name exercised.

  Background:
    Given the server is running with engine "trino-local" registered

  Scenario: A bare word suggests catalogs alongside keywords
    When subject "alice" asks for completion after "select * from pol"
    Then the suggestions include catalog "polaris-local"
    And the catalog "polaris-local" is suggested quoted

  Scenario: A catalog prefix suggests its schemas
    When subject "alice" asks for completion after "select * from polaris-local."
    Then the suggestions include schema "default"

  Scenario: A quoted catalog prefix suggests its schemas too
    When subject "alice" asks for completion after 'select * from "polaris-local".'
    Then the suggestions include schema "default"

  Scenario: An unknown catalog suggests nothing rather than failing
    When subject "alice" asks for completion after "select * from nope."
    Then the response status is 200
    And the suggestions are empty

  Scenario: A schema prefix suggests its tables
    When subject "alice" asks for completion after "select * from polaris-local.default."
    Then the suggestions include table "orders"

  Scenario: A table prefix suggests its columns
    When subject "alice" asks for completion after "select order_id, total from polaris-local.default.orders."
    Then the suggestions include column "order_id"
    And the suggestions include column "total"

  Scenario: A prefix that matches no column suggests nothing
    When subject "alice" asks for completion after "select * from polaris-local.default.orders.nope"
    Then the response status is 200
    And the suggestions are empty
