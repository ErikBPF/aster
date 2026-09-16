{ pkgs, lib, config, inputs, ... }:

{
  dotenv.enable = true;

  packages = [
    # rust toolchain: pinned by the declared devenv, not by a host install
    pkgs.cargo
    pkgs.rustc
    pkgs.rustfmt
    pkgs.clippy
    pkgs.rust-analyzer

    # workflows and glue
    pkgs.git
    pkgs.just
    pkgs.jq
    pkgs.curl
    pkgs.ripgrep
    pkgs.shellcheck
    pkgs.python3

    # containers
    pkgs.podman
    pkgs.docker-compose

    # local kubernetes loop
    pkgs.kubectl
    pkgs.kubernetes-helm
    pkgs.kubeconform
    pkgs.devspace
    pkgs.minikube
  ];

  enterShell = ''
    echo "=== aster — git-backed SQL notebooks ==="
    echo "  rust:  $(cargo --version 2>/dev/null)"
    echo "  helm:  $(helm version --short 2>/dev/null)"
    echo "  just:  $(just --version 2>/dev/null)"
    echo "  try:   just ci | just compose-up | just chart-lint"
  '';

  # `devenv test` is the gate CI runs; it delegates to the justfile so the
  # recipes stay the single source of truth.
  enterTest = ''
    just ci
  '';
}
