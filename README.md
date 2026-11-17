# publictransport

A client library for public transport APIs. One shared model for stops, departures and trips covers several server types, so an application can query any supported network the same way.

The project is at an early stage. It can search for locations by name, list the departures at a stop and plan trips between two places on servers that speak one of four protocols:

- EFA, the JSON interface of MDV's journey planner, used by many German transport associations
- MOTIS, the routing engine behind [Transitous](https://transitous.org)
- TRIAS 1.2, the German XML standard VDV 431-2
- OJP 1.0, its European successor

The library ships with a small registry of networks that were checked against their live servers, `Registry::bundled()`. Further networks can be described in code or read from entries in the format of [transport-apis](https://github.com/public-transport/transport-apis). Each network says where it has coverage, so an application can pick the networks for a position.

## Design

The core crate does no I/O. It builds HTTP requests and parses HTTP responses, and the caller sends them. This keeps the core usable from other languages and from WebAssembly, where each platform brings its own HTTP stack. The crate documentation of `publictransport` shows the request and parse steps and what the HTTP client has to do.

Rust applications that do not want to bring their own HTTP client use `publictransport-client`, which sends the requests with `reqwest`. Its example plans a trip against a live server:

```sh
cargo run -p publictransport-client --example trip -- vvo-efa "Dresden Hauptbahnhof" "Dresden Albertplatz"
```

## Development

Clone with `--recurse-submodules`, or run `git submodule update --init` afterwards: the network data of [transport-apis](https://github.com/public-transport/transport-apis) is a submodule under `crates/core/data`. Networks that repository does not have yet are in `crates/core/data/networks`, in the same format.

The development environment is a nix flake; `direnv allow` or `nix develop` provides the toolchain. CI runs these checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

A Python package in `packages/python` wraps the library through [UniFFI](https://mozilla.github.io/uniffi-rs/); see its README. The crate in `bindings/uniffi` builds the shared library for it.

The tests replay responses recorded from real servers. `crates/core/tests/fixtures/record.sh` records them again. `scripts/validate-xml-requests.sh` checks the TRIAS and OJP requests that the library builds against the official schemas, which it fetches from the VDV repositories.

`CONTRIBUTING.md` describes the commit message format and how the changelog is made.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
