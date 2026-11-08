# Minimal CI shell - just Rust toolchain
# Uses vanilla mkShell (not devenv framework)
{
  pkgs,
  inputs,
  system,
}: let
  rustnix = inputs.rustnix.lib.rust;

  # Minimal stable Rust toolchain for CI
  toolchain = rustnix.mkToolchain {inherit system;};
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
