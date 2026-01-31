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
    {
      nixosModules.nix_autobuild = ((import ./autobuildModule.nix) self);
    }
    // (
      flake-utils.lib.eachDefaultSystem (system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };
        naersk' = pkgs.callPackage naersk {};

        rust-bin = pkgs.rust-bin.nightly."2025-04-15".default.override {
          extensions = ["rust-analyzer" "rust-src" "clippy" "llvm-tools"];
          targets = ["wasm32-unknown-unknown"];
        };

        buildInputs = [
          pkgs.pkg-config
          pkgs.openssl
          pkgs.glibc
          pkgs.llvmPackages.lld
          rust-bin
        ];

        wasm-bindgen-cli = pkgs.buildWasmBindgenCli rec {
          src = pkgs.fetchCrate {
            pname = "wasm-bindgen-cli";
            version = "0.2.106";
            hash = "sha256-M6WuGl7EruNopHZbqBpucu4RWz44/MSdv6f0zkYw+44=";
          };

          cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
            inherit src;
            inherit (src) pname version;
            hash = "sha256-ElDatyOwdKwHg3bNH/1pcxKI7LXkhsotlDPQjiLHBwA=";
          };
        };

        frontend = naersk'.buildPackage {
          src = ./.;
          buildInputs = buildInputs;
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
          postInstall = ''
            ${wasm-bindgen-cli}/bin/wasm-bindgen --out-dir $out/dist --target web $out/bin/nix_autobuild.wasm
            cp -r $src/index.html $out/dist/
            cp -r $src/styles.css $out/dist/
          '';
        };

        backend = naersk'.buildPackage {
          src = ./.;
          buildInputs = buildInputs;
          FRONTEND_PATH = "${frontend}/dist";
        };
      in {
        packages.backend = backend;
        packages.frontend = frontend;
        packages.default = backend;

        devShells.default = pkgs.mkShell {
          buildInputs =
            [
              pkgs.trunk
              rust-bin
              wasm-bindgen-cli
            ]
            ++ buildInputs;
        };

        formatter = pkgs.alejandra;
      })
    );
}
