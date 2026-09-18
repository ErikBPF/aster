@contract
# Behavior contract for the server-rendered shell.
#
# Bound to `crates/server/tests/contracts.rs`, which drives the router in
# process; the same pages are also exercised against a live server in the
# README smoke steps. Look-and-feel details (CSS, keyboard shortcuts) are not
# asserted here; these scenarios protect the structure the script needs.

Feature: Web shell

  Background:
    Given the server is running with engine "trino-local" registered
    And subject "alice" has saved notebook "sales" with one cell "SELECT 1"

  Scenario: Every page is framed by the shell
    When subject "alice" opens the index page
    Then the response status is 200
    And the page carries the application shell

  Scenario: The notebook page renders an editor and a run action per cell
    When subject "alice" opens the "sales" notebook page
    Then the response status is 200
    And the page renders one editor and one run action per cell

  Scenario: Notebook text cannot break out of the page
    Given subject "alice" has saved notebook "alert" titled "<script>alert(1)</script>"
    When subject "alice" opens the "alert" notebook page
    Then the response status is 200
    And the notebook title is escaped in the markup

  Scenario: An unidentified caller is sent to sign in
    When a caller without identity opens the index page
    Then the response status is 303
