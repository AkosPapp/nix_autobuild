{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
    naersk,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };
      naersk' = pkgs.callPackage naersk {};
    in {
      packages.default = naersk'.buildPackage {
        src = ./.;
      };

      devShells.default = pkgs.mkShell {
        buildInputs = [
          (pkgs.rust-bin.nightly."2025-04-15".default.override {
            extensions = ["rust-analyzer" "rust-src" "clippy" "llvm-tools"];
          })
        ];
      };

      formatter = pkgs.alejandra;
    });
}
