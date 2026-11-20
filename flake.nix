{
  description = "publictransport";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      formatter = forAllSystems (pkgs: pkgs.nixfmt);

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          # Use the Python of this shell, not one that uv downloads.
          UV_PYTHON_PREFERENCE = "only-system";
          packages = with pkgs; [
            (rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
              targets = [
                # The WebAssembly module of the TypeScript package
                "wasm32-unknown-unknown"
              ]
              # For the XCFramework of the Swift package, see
              # scripts/build-swift.sh
              ++ lib.optionals stdenv.hostPlatform.isDarwin [
                "aarch64-apple-darwin"
                "x86_64-apple-darwin"
                "aarch64-apple-ios"
                "aarch64-apple-ios-sim"
                "x86_64-apple-ios"
                "aarch64-apple-tvos"
                "aarch64-apple-tvos-sim"
                "aarch64-apple-watchos"
                "aarch64-apple-watchos-sim"
                "aarch64-apple-visionos"
                "aarch64-apple-visionos-sim"
              ];
            })
            cargo-deny
            # Compares the API with the released crates, for scripts/prepare-release.sh
            cargo-semver-checks
            # The commands in the Justfile
            just
            # The commit convention, see CONTRIBUTING.md
            committed
            git-cliff
            # Builds the documentation site in `site`
            mdbook
            # Finds OpenSSL for the native-tls feature of the client.
            pkg-config
            # xmllint and the shell, for scripts/validate-xml-requests.sh
            libxml2
            zsh
            # The Python package in packages/python
            python3
            uv
            ruff
            pyright
            # The Kotlin package in packages/kotlin. Gradle brings its own JDK.
            gradle_9
            ktlint
            # The TypeScript package in packages/typescript. The tool that
            # writes the JavaScript side of the WebAssembly module has to
            # have the version of the wasm-bindgen crate in Cargo.lock, which
            # packages/typescript/build.sh checks.
            wasm-bindgen-cli
            # wasm-opt
            binaryen
            nodejs
            pnpm
            biome
          ];
          # On Linux the native-tls feature of the client links OpenSSL.
          buildInputs = [ pkgs.openssl ];
        };
      });
    };
}
