# Development shell with full tooling
# Uses vanilla mkShell (not devenv framework)
{
  pkgs,
  inputs,
  system,
}: let
  fenix = inputs.fenix.packages.${system};

  # Stable Rust toolchain with essential components
  toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.stable.rust-analyzer
    fenix.stable.clippy
    fenix.stable.rustfmt
    fenix.stable.llvm-tools-preview
  ];

  # Helper scripts as shell functions
  helperScripts = ''
    # Helper functions
    check() {
      cargo check "$@"
    }

    fmt() {
      cargo fmt "$@"
    }

    tests() {
      cargo test "$@"
    }

    clippy() {
      cargo clippy --all-targets --all-features -- -D warnings "$@"
    }

    coverage() {
      cargo llvm-cov --html --open "$@"
    }

    build() {
      cargo build --release "$@"
    }

    # Print available commands
    help-bogita() {
      echo ""
      echo "Bogita Development Shell"
      echo "========================"
      echo ""
      echo "Helper commands:"
      echo "  check      - Run cargo check"
      echo "  fmt        - Run cargo fmt"
      echo "  tests      - Run cargo test"
      echo "  clippy     - Run cargo clippy (strict)"
      echo "  coverage   - Generate code coverage report"
      echo "  build      - Build release binary"
      echo ""
      echo "Tools available:"
      echo "  sqlx       - SQLx CLI for migrations"
      echo "  age        - Encryption tool"
      echo "  prek       - Git hooks manager"
      echo ""
    }
  '';
in
  pkgs.mkShell {
    name = "bogita-dev";

    buildInputs = [
      # Rust toolchain
      toolchain

      # Development tools
      pkgs.sqlx-cli # Database migrations
      pkgs.cargo-tarpaulin # Code coverage (alternative)
      pkgs.cargo-llvm-cov # Code coverage (primary)

      # Security tools
      pkgs.age # Encryption
      pkgs.openssh # SSH utilities
      pkgs.git # Git operations

      # Git hooks
      pkgs.prek
    ];

    shellHook = ''
      ${helperScripts}

      # Initialize prek on shell entry
      if [ -f prek.toml ]; then
        prek install -t pre-commit -t pre-push
      fi

      # Welcome message
      help-bogita
    '';
  }
