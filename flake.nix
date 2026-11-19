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
          packages = [
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
              # For the XCFramework of the Swift package, see
              # scripts/build-swift.sh
              targets = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
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
            pkgs.cargo-deny
            # Compares the API with the released crates, for scripts/prepare-release.sh
            pkgs.cargo-semver-checks
            # The commands in the Justfile
            pkgs.just
            # The commit convention, see CONTRIBUTING.md
            pkgs.committed
            pkgs.git-cliff
            # Finds OpenSSL for the native-tls feature of the client.
            pkgs.pkg-config
            # xmllint and the shell, for scripts/validate-xml-requests.sh
            pkgs.libxml2
            pkgs.zsh
            # The Python package in packages/python
            pkgs.python3
            pkgs.uv
            pkgs.ruff
            pkgs.pyright
          ];
          # On Linux the native-tls feature of the client links OpenSSL.
          buildInputs = [ pkgs.openssl ];
        };
      });
    };
}
