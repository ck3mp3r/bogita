# Packaging logic for bogita
# Uses rustnix.lib.rust.buildTargetOutputs pattern
{
  inputs,
  system,
  pkgs,
  cargoToml,
  cargoLock,
  overlays,
}: let
  supportedTargets = ["aarch64-darwin" "aarch64-linux" "x86_64-darwin" "x86_64-linux"];

  # Install data for pre-built releases
  # These will be populated after first release
  installData = {
    aarch64-darwin = builtins.fromJSON (builtins.readFile ../data/aarch64-darwin.json);
    aarch64-linux = builtins.fromJSON (builtins.readFile ../data/aarch64-linux.json);
    x86_64-darwin = builtins.fromJSON (builtins.readFile ../data/x86_64-darwin.json);
    x86_64-linux = builtins.fromJSON (builtins.readFile ../data/x86_64-linux.json);
  };

  # Build regular packages (no archives)
  regularPackages = inputs.rustnix.lib.rust.buildTargetOutputs {
    inherit
      cargoToml
      cargoLock
      overlays
      pkgs
      system
      installData
      supportedTargets
      ;
    fenix = inputs.fenix;
    nixpkgs = inputs.nixpkgs;
    src = ../.;
    packageName = "bogita";
    archiveAndHash = false;
  };

  # Build archive packages (creates archive with system name)
  archivePackages = inputs.rustnix.lib.rust.buildTargetOutputs {
    inherit
      cargoToml
      cargoLock
      overlays
      pkgs
      system
      installData
      supportedTargets
      ;
    fenix = inputs.fenix;
    nixpkgs = inputs.nixpkgs;
    src = ../.;
    packageName = "archive";
    archiveAndHash = true;
  };
in {
  # Merge both package sets
  packages = regularPackages // archivePackages;

  # Default app points to the main binary
  apps = {
    default = {
      type = "app";
      program = "\${regularPackages.default}/bin/bogita";
    };
  };
}
