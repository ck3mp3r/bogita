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
      pkgs.cargo-nextest # test runner: .config/nextest.toml profiles, pre-push hook entry
      # The ssh_sign roundtrip tests shell out to ssh-keygen; without it the
      # full suite cannot pass in this shell (devenv.nix declares it too).
      pkgs.openssh
    ];

    shellHook = ''
      echo "Bogita CI Environment"
      echo "Rust: $(rustc --version)"
      echo ""
    '';
  }
