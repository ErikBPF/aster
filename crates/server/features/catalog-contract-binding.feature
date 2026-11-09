@contract @unautomated
# Draft Gherkin; executable regression: odcs_resolution::catalog_detail_uses_exact_contract_binding.
Feature: Catalog detail uses the selected physical binding
  Scenario: Admitted same-named objects do not replace the selected table's contract
    Given two admitted contracts declare a table named "orders" in different catalogs
    When Alice opens the detail for the orders table in warehouse
    Then its permissions identify the contract bound to that exact catalog and namespace
