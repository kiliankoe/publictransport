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

# Run the examples of every language against their live servers
examples: examples-rust examples-python examples-swift

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

# Ask the live server of every bundled network for stops, departures and trips
check-networks:
    cargo run -p publictransport-client --example check

# Build the documentation site into site/book. mdBook only warns about a file it cannot include and says nothing about a missing anchor, so both are checked here
site:
    for file in README.md packages/python/README.md packages/swift/README.md; do \
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
