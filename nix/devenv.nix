# Development shell with full tooling
# Uses vanilla mkShell (not devenv framework)
{
  pkgs,
  inputs,
  system,
}: let
  rustnix = inputs.rustnix.lib.rust;

  # Stable Rust toolchain with dev components, from rustnix
  # (includes the Darwin rustc LLVM dylib fix and the host target std)
  toolchain = rustnix.mkToolchain {
    inherit system;
    extras = ["clippy" "rustfmt" "rust-analyzer" "llvm-tools-preview"];
  };

  # nixpkgs cargo-llvm-cov runs its test suite at build time and fails under
  # this toolchain; skip tests like rustnix.devShells.coverage does
  cargoLlvmCov = pkgs.cargo-llvm-cov.overrideAttrs (_: {
    doCheck = false;
  });

  # Helper commands as real executables on PATH: they must work in any shell
  # (nushell via direnv included), where bash functions from shellHook cannot exist
  helperHelpText = ''
    echo ""
    echo "Bogita Development Shell"
    echo "========================"
    echo ""
    echo "Helper commands:"
    echo "  check      - Run cargo check"
    echo "  fmt        - Run cargo fmt"
    echo "  tests      - Run unit tests (cargo nextest)"
    echo "  tests-all  - Run unit + integration tests"
    echo "  tests-integration - Run integration tests with retries"
    echo "  clippy     - Run cargo clippy (strict)"
    echo "  coverage   - Generate code coverage report"
    echo "  build      - Build release binary"
    echo ""
    echo "Tools available:"
    echo "  sqlx       - SQLx CLI for migrations"
    echo "  age        - Encryption tool"
    echo "  prek       - Git hooks manager"
    echo ""
  '';

  helpers = [
    (pkgs.writeShellScriptBin "check" ''exec cargo check "$@"'')
    (pkgs.writeShellScriptBin "fmt" ''exec cargo fmt "$@"'')
    (pkgs.writeShellScriptBin "tests" ''exec cargo nextest run "$@"'')
    (pkgs.writeShellScriptBin "tests-all" ''exec cargo nextest run --features integration "$@"'')
    (pkgs.writeShellScriptBin "tests-integration" ''exec cargo nextest run --features integration --profile integration "$@"'')
    (pkgs.writeShellScriptBin "clippy" ''exec cargo clippy --all-targets --all-features -- -D warnings "$@"'')
    (pkgs.writeShellScriptBin "coverage" ''exec cargo llvm-cov --html --open "$@"'')
    (pkgs.writeShellScriptBin "build" ''exec cargo build --release "$@"'')
    (pkgs.writeShellScriptBin "help-bogita" helperHelpText)
  ];

  helpersPath = pkgs.lib.makeBinPath helpers;
in
  pkgs.mkShell {
    name = "bogita-dev";

    buildInputs =
      helpers
      ++ [
        # Rust toolchain
        toolchain

        # Development tools
        pkgs.sqlx-cli # Database migrations
        pkgs.cargo-tarpaulin # Code coverage (alternative)
        cargoLlvmCov # Code coverage (primary)
        pkgs.cargo-nextest # Test runner (unit + integration profiles)

        # Security tools
        pkgs.age # Encryption
        pkgs.openssh # SSH utilities
        pkgs.git # Git operations

        # Git hooks
        pkgs.prek
      ]
      ++ helpers;

    shellHook = ''
      # Helpers must win over same-named binaries (e.g. GNU fmt from coreutils)
      export PATH="${helpersPath}:$PATH"

      # Initialize prek on shell entry
      if [ -f prek.toml ]; then
        prek install -t pre-commit -t pre-push
      fi

      # Welcome message
      help-bogita
    '';
  }
