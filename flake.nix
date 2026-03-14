{
  description = "wasm_utils";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-25.11";
    nixpkgs-unstable.url = "nixpkgs/nixpkgs-unstable";

    command-utils.url = "git+https://codeberg.org/expede/nix-command-utils";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    command-utils,
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
          ];

          targets = [
            "aarch64-apple-darwin"
            "x86_64-apple-darwin"

            "x86_64-unknown-linux-musl"
            "aarch64-unknown-linux-musl"

            "wasm32-unknown-unknown"
          ];
        };

        # Nightly rustfmt for unstable formatting options (imports_granularity, etc.)
        # rustfmt links against librustc_driver; on macOS symlinks break @rpath
        # resolution, so we wrap the binary with DYLD_LIBRARY_PATH.
        nightly-rustfmt-unwrapped = pkgs.rust-bin.nightly.latest.minimal.override {
          extensions = [ "rustfmt" ];
        };

        nightly-rustfmt = pkgs.writeShellScriptBin "rustfmt" ''
          export DYLD_LIBRARY_PATH="${nightly-rustfmt-unwrapped}/lib''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
          export LD_LIBRARY_PATH="${nightly-rustfmt-unwrapped}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          exec "${nightly-rustfmt-unwrapped}/bin/rustfmt" "$@"
        '';

        format-pkgs = [
          pkgs.nixpkgs-fmt
          pkgs.alejandra
          pkgs.taplo
        ];

        cargo-installs = [
          pkgs.cargo-component
          pkgs.cargo-criterion
          pkgs.cargo-deny
          pkgs.cargo-expand
          pkgs.cargo-nextest
          pkgs.cargo-outdated
          pkgs.cargo-sort
          pkgs.cargo-udeps
          pkgs.cargo-watch
          pkgs.twiggy
          pkgs.wasm-bindgen-cli
          pkgs.wasm-tools
        ];

        # Built-in command modules from nix-command-utils
        rust = command-utils.rust.${system};
        wasm = command-utils.wasm.${system};
        cmd = command-utils.cmd.${system};

        command_menu = command-utils.commands.${system} [
          # Rust commands
          (rust.build { cargo = pkgs.cargo; })
          (rust.test { cargo = pkgs.cargo; cargo-watch = pkgs.cargo-watch; })
          (rust.lint { cargo = pkgs.cargo; })
          (rust.fmt { cargo = pkgs.cargo; })
          (rust.doc { cargo = pkgs.cargo; })
          (rust.watch { cargo-watch = pkgs.cargo-watch; })

          # Wasm commands
          (wasm.build { wasm-pack = pkgs.wasm-pack; })
          (wasm.test { wasm-pack = pkgs.wasm-pack; })
          (wasm.doc { cargo = pkgs.cargo; xdg-open = pkgs.xdg-utils; })
        ];

      in rec {
        devShells.default = pkgs.mkShell {
          name = "wasm_utils shell";

          nativeBuildInputs = with pkgs;
            [
              command_menu
              nightly-rustfmt
              rust-toolchain

              http-server
              pkgs.binaryen
              pkgs.nodePackages_latest.webpack-cli
              pkgs.nodejs_22
              pkgs.rust-analyzer
              pkgs.wasm-pack
            ]
            ++ format-pkgs
            ++ cargo-installs;

          shellHook = ''
            export WORKSPACE_ROOT="$(pwd)"
            export RUSTFMT="${nightly-rustfmt}/bin/rustfmt"
            menu
          '';
        };

        formatter = pkgs.alejandra;
      }
    );
}
