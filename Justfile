# The commands of the project. Run them in the development shell
# (`nix develop` or direnv); `just` alone lists them.

# Stop at the first failing command, also inside a loop.
set shell := ["bash", "-euc"]

_default:
    @just --list

# Everything CI checks in the Rust code
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    cargo clippy -p publictransport --all-targets -- -D warnings
    cargo test -p publictransport
    cargo deny check
    scripts/validate-xml-requests.sh
    scripts/build-swift.sh check

# Lint, type-check and test the Python package
python:
    cd packages/python && uv sync --quiet --reinstall-package publictransport
    cd packages/python && ruff check . && ruff format --check . && pyright && uv run --no-sync pytest

# Write the generated Swift code again, after a change to the Rust library
swift-bindings:
    scripts/build-swift.sh bindings

# Build the Rust library for the Swift package: for macOS, or with `all` for every Apple platform
swift-library platforms="macos":
    scripts/build-swift.sh xcframework {{ if platforms == "all" { "" } else { platforms } }}

# Test the Swift package against the library that `swift-library` built
swift-test:
    # Xcode's Swift does not run with the SDK that the development shell sets.
    env -i PATH=/usr/bin:/bin:/usr/sbin HOME="$HOME" PUBLICTRANSPORT_LOCAL_BINARY=1 swift test
    env -i PATH=/usr/bin:/bin:/usr/sbin HOME="$HOME" swift format lint --strict --recursive \
        packages/swift/Sources/PublicTransport/Client.swift packages/swift/Sources/PublicTransport/Codable.swift packages/swift/Tests

# Lint and test the Kotlin package and compile its examples, with the Rust library built for them
kotlin:
    ktlint 'packages/kotlin/**/*.kt' 'packages/kotlin/**/*.kts' 'examples/kotlin/**/*.kt' 'examples/kotlin/**/*.kts' '!**/build/**'
    gradle --project-dir packages/kotlin --quiet build

# Build and lint the Android library and the app that tests it, in an Android shell such as `nix develop .#android`
android:
    gradle --project-dir packages/kotlin --quiet :android:build :android:smoke:build :android:smoke:assembleAndroidTest

# Run the test of the Android library on a connected device or a running emulator, after `just android`
android-test:
    scripts/android-test.sh

# Lint, build, type-check and test the TypeScript package, with the Rust library built for it as WebAssembly
typescript:
    cd packages/typescript && pnpm install --frozen-lockfile --silent
    cd examples/typescript && pnpm install --frozen-lockfile --silent
    cd packages/typescript && pnpm lint && pnpm build && pnpm typecheck && pnpm test

# Run the examples of every language against their live servers
examples: examples-rust examples-python examples-swift examples-kotlin examples-typescript

# Run the Rust examples
examples-rust:
    for name in locations departures trips networks; do cargo run --quiet -p publictransport-examples --bin $name; done

# Lint and run the Python examples
examples-python:
    cd packages/python && uv sync --quiet --reinstall-package publictransport
    cd packages/python && ruff check ../../examples/python && ruff format --check ../../examples/python && pyright ../../examples/python
    for name in locations departures trips networks; do uv run --project packages/python --no-sync python examples/python/$name.py; done

# Lint and run the Swift examples against the library that `swift-library` built
examples-swift:
    env -i PATH=/usr/bin:/bin:/usr/sbin HOME="$HOME" swift format lint --strict --recursive \
        --configuration packages/swift/.swift-format examples/swift
    for name in locations departures trips networks; do \
        env -i PATH=/usr/bin:/bin:/usr/sbin HOME="$HOME" PUBLICTRANSPORT_LOCAL_BINARY=1 \
            swift run --quiet --package-path examples/swift $name; \
    done

# Run the Kotlin examples
examples-kotlin:
    for name in locations departures trips networks; do gradle --project-dir packages/kotlin --quiet :examples:$name; done

# Build the TypeScript package and run its examples
examples-typescript:
    cd packages/typescript && pnpm install --frozen-lockfile --silent && pnpm build
    cd examples/typescript && pnpm install --frozen-lockfile --silent
    for name in locations departures trips networks; do node examples/typescript/$name.ts; done

# Ask the live servers of the bundled networks for stops, departures and trips: every one but Transitous, or the ones named
check-networks *networks:
    cargo run -p publictransport-client --example check -- {{networks}}

# Build the documentation site into site/book. mdBook only warns about a file it cannot include and says nothing about a missing anchor, so both are checked here
site:
    for file in README.md packages/python/README.md packages/swift/README.md packages/kotlin/README.md packages/typescript/README.md; do \
        grep -q "ANCHOR: site" $file; grep -q "ANCHOR_END: site" $file; \
    done
    cargo run --quiet -p publictransport --example network_table > site/src/networks.md
    set -o pipefail; mdbook build site 2>&1 | tee /dev/stderr | (! grep -q "WARN\|ERROR")

# Show the documentation site in a browser. It follows changes in site/src, not in the files it includes
site-serve: site
    mdbook serve --open site

# Prepare a release, see CONTRIBUTING.md: set the version and write the changelog. Without a version, the next one by the commits
release version="":
    scripts/prepare-release.sh {{ version }}
