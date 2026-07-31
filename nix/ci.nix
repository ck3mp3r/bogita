# Minimal CI shell - just Rust toolchain
# Uses vanilla mkShell (not devenv framework)
{
  pkgs,
  inputs,
  system,
}: let
  fenix = inputs.fenix.packages.${system};

  # Minimal stable Rust toolchain for CI
  toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
  ];
in
  pkgs.mkShell {
    name = "bogita-ci";

    buildInputs = [
      toolchain
    ];

    shellHook = ''
      echo "Bogita CI Environment"
      echo "Rust: $(rustc --version)"
      echo ""
    '';
  }
