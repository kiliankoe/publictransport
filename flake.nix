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
      # What the Android library in packages/kotlin/android is built with.
      # The versions are also in its build.gradle.kts: the Android Gradle
      # Plugin cannot install another one into the store.
      androidVersions = {
        platform = "37";
        buildTools = "37.0.0";
        ndk = "30.0.16248370";
      };
      androidTargets = [
        "aarch64-linux-android"
        "armv7-linux-androideabi"
        "x86_64-linux-android"
      ];
    in
    {
      formatter = forAllSystems (pkgs: pkgs.nixfmt);

      devShells = forAllSystems (
        pkgs:
        let
          # The Android SDK is unfree. Only the `android` shell takes it.
          unfree = import nixpkgs {
            inherit (pkgs.stdenv.hostPlatform) system;
            config = {
              allowUnfree = true;
              android_sdk.accept_license = true;
            };
          };
          # `android` adds what the Android library needs besides an SDK,
          # `sdk` the SDK itself.
          shell =
            {
              android ? false,
              sdk ? null,
            }:
            pkgs.mkShell (
              {
                # Use the Python of this shell, not one that uv downloads.
                UV_PYTHON_PREFERENCE = "only-system";
                packages =
                  with pkgs;
                  [
                    (rust-bin.stable.latest.default.override {
                      extensions = [
                        "rust-src"
                        "rust-analyzer"
                      ];
                      targets = [
                        # The WebAssembly module of the TypeScript package
                        "wasm32-unknown-unknown"
                      ]
                      ++ lib.optionals android androidTargets
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
                  ]
                  # Builds the Rust library for the Android ABIs
                  ++ lib.optional android cargo-ndk
                  ++ lib.optional (sdk != null) sdk;
                # On Linux the native-tls feature of the client links OpenSSL.
                buildInputs = [ pkgs.openssl ];
              }
              // pkgs.lib.optionalAttrs android {
                # Makes the Android library a part of the Gradle build, see
                # packages/kotlin/settings.gradle.kts.
                ORG_GRADLE_PROJECT_android = "true";
              }
              // pkgs.lib.optionalAttrs (android && sdk == null) {
                # cargo-ndk takes the NDK from this variable, which names
                # another version on GitHub's runners.
                shellHook = ''
                  export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/${androidVersions.ndk}"
                  if [ ! -d "$ANDROID_NDK_HOME" ]; then
                    echo "The SDK in ANDROID_HOME has no NDK ${androidVersions.ndk}" >&2
                    exit 1
                  fi
                '';
              }
              // pkgs.lib.optionalAttrs (sdk != null) rec {
                ANDROID_HOME = "${sdk}/libexec/android-sdk";
                # GitHub's runners set this one to their own SDK, and the Android
                # Gradle Plugin refuses two.
                ANDROID_SDK_ROOT = ANDROID_HOME;
                ANDROID_NDK_HOME = "${ANDROID_HOME}/ndk/${androidVersions.ndk}";
                # The aapt2 that the Android Gradle Plugin downloads does not run on NixOS.
                GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${ANDROID_HOME}/build-tools/${androidVersions.buildTools}/aapt2";
              }
            );
        in
        {
          default = shell { };
          # The default shell with the Android SDK and NDK, for the Android
          # library: `nix develop .#android`. Entering it accepts the license
          # of the SDK.
          android = shell {
            android = true;
            sdk =
              (unfree.androidenv.composeAndroidPackages {
                platformVersions = [ androidVersions.platform ];
                buildToolsVersions = [ androidVersions.buildTools ];
                includeNDK = true;
                ndkVersions = [ androidVersions.ndk ];
              }).androidsdk;
          };
          # The same for a machine that has these versions of the SDK and
          # the NDK itself, under ANDROID_HOME. CI uses it, since GitHub's
          # runners have them and building the SDK takes minutes.
          android-host-sdk = shell { android = true; };
        }
      );
    };
}
