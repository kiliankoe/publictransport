# publictransport

A client library for public transport APIs. One shared model for stops, departures and trips covers several server types, so an application can query any supported network the same way.

The project is at an early stage. It can search for locations by name, list the departures at a stop and plan trips between two places on servers that speak EFA (the JSON interface of MDV's journey planner, used by many German transport associations) or MOTIS (the routing engine behind [Transitous](https://transitous.org)).

## Design

The core crate does no I/O. It builds HTTP requests and parses HTTP responses, and the caller sends them. This keeps the core usable from other languages and from WebAssembly, where each platform brings its own HTTP stack. The crate documentation of `publictransport` shows the request and parse steps and what the HTTP client has to do.

Rust applications that do not want to bring their own HTTP client use `publictransport-client`, which sends the requests with `reqwest`. Its example plans a trip against a live server:

```sh
cargo run -p publictransport-client --example trip -- vvo "Dresden Hauptbahnhof" "Dresden Albertplatz"
```

## Development

The development environment is a nix flake; `direnv allow` or `nix develop` provides the toolchain. CI runs these checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

A Python package in `packages/python` wraps the library through [UniFFI](https://mozilla.github.io/uniffi-rs/); see its README. The crate in `bindings/uniffi` builds the shared library for it.

The tests replay responses recorded from real servers. `crates/core/tests/fixtures/record.sh` records them again.

`CONTRIBUTING.md` describes the commit message format and how the changelog is made.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
