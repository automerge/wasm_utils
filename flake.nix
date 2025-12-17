{
  description = "wasm_utils";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-25.11";
    nixpkgs-unstable.url = "nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = {
    self,
    flake-utils,
    nixpkgs,
    nixpkgs-unstable,
    rust-overlay,
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [
          (import rust-overlay)
        ];

        pkgs = import nixpkgs {
          inherit system overlays;
          config.allowUnfree = true;
        };

        unstable = import nixpkgs-unstable {
          inherit system overlays;
          config.allowUnfree = true;
        };

        rustVersion = "1.90.0";

        rust-toolchain = pkgs.rust-bin.stable.${rustVersion}.default.override {
          extensions = [
            "cargo"
            "clippy"
            "llvm-tools-preview"
            "rust-src"
            "rust-std"
            "rustfmt"
          ];

          targets = [
            "aarch64-apple-darwin"
            "x86_64-apple-darwin"

            "x86_64-unknown-linux-musl"
            "aarch64-unknown-linux-musl"

            "wasm32-unknown-unknown"
          ];
        };

        format-pkgs = [
          pkgs.nixpkgs-fmt
          pkgs.alejandra
          pkgs.taplo
        ];

        cargo-installs =  [
          pkgs.cargo-criterion
          pkgs.cargo-deny
          pkgs.cargo-expand
          pkgs.cargo-nextest
          pkgs.cargo-outdated
          pkgs.cargo-sort
          pkgs.cargo-udeps
          pkgs.cargo-watch
          pkgs.twiggy
          pkgs.cargo-component
          pkgs.wasm-bindgen-cli
          pkgs.wasm-tools
        ];

      in rec {
        devShells.default = pkgs.mkShell {
          name = "wasm_utils shell";

          nativeBuildInputs = with pkgs;
            [
              rust-toolchain
              pkgs.irust

              http-server
              pkgs.binaryen
              pkgs.nodePackages_latest.webpack-cli
              pkgs.nodejs_22
              pkgs.rust-analyzer
              pkgs.wasm-pack
            ]
            ++ format-pkgs
            ++ cargo-installs;
        };

        formatter = pkgs.alejandra;
      }
    );
}
