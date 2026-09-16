# Repository conventions are themselves a contract: these scenarios are executed
# by tests/repo-setup.sh, which `just ci` runs. They assert tooling wiring, not
# product behavior — anything that describes what aster does belongs in
# crates/<crate>/features/.
@contract
Feature: Repository conventions and starting setup

  The toolchain, the gate, the local stacks and the chart are declared in the
  repository and validated, so a new contributor (or agent) inherits the same
  working environment instead of rediscovering it.

  Scenario: The toolchain is declared, not assumed
    Given there is no globally installed Rust toolchain to rely on
    Then devenv.yaml requests the rolling devenv nixpkgs input
    And devenv.nix provides cargo, rustc, rustfmt, clippy and just
    And rust-toolchain.toml pins the channel editors should use

  Scenario: One command is the gate
    When a contributor runs the gate
    Then it checks formatting, lints, tests, behavior contracts and the repository contract
    And the devenv test entry point runs the same gate

  Scenario: Behavior contracts sit next to their code
    Then every feature file declares a Feature and at least one Scenario
    And crate behavior files live under crates/<crate>/features/
    And repository conventions live under features/

  Scenario: The local stack is declared and starts
    Then docker-compose.yml defines postgres, the server and the controller
    And the compose file is valid YAML for the container engine
    And the server answers the health endpoint once the stack is up

  Scenario: The chart renders valid Kubernetes resources
    Then the chart declares Chart.yaml, values.yaml and values.schema.json
    And every rendered resource validates against the Kubernetes schemas
    And the values schema requires a Harbor repository and an immutable digest

  Scenario: Secrets never reach the repository or the values file
    Then no tracked *.secrets.json or .env file exists
    And the chart takes secrets from an ExternalSecret rather than values
