@contract
Feature: Traces and metrics
  # The metrics listener is deliberately separate from the public router and
  # unauthenticated: Prometheus cannot carry a session cookie, so the port is
  # gated by NetworkPolicy instead. crates/server/tests/contracts.rs executes
  # these scenarios in process; the OTLP exporter is exercised against a real
  # collector in deployment (OTEL_EXPORTER_OTLP_ENDPOINT).

  Scenario: Scraping the metrics listener reports request counters
    Given the server is running with engine "trino-local" registered
    When the metrics listener is scraped
    Then the scrape names "aster_http_requests_total"
    And the scrape names "aster_http_request_seconds"
    And the scrape carries a route label

  Scenario: The public router does not serve metrics
    Given the server is running with engine "trino-local" registered
    When metrics are requested from the public router
    Then the call does not succeed
