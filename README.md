# publictransport

A client library for public transport APIs. One shared model for stops, departures and trips covers several server types, so an application can query any supported network the same way.

The project is at an early stage. It can search for locations by name, list the departures at a stop and plan trips between two places on servers that speak EFA (the JSON interface of MDV's journey planner, used by many German transport associations) or MOTIS (the routing engine behind [Transitous](https://transitous.org)).

## Design

The core crate does no I/O. It builds HTTP requests and parses HTTP responses, and the caller sends them. This keeps the core usable from other languages and from WebAssembly, where each platform brings its own HTTP stack. A separate client crate will do the HTTP calls for Rust users. The crate documentation of `publictransport` shows the request and parse steps and what the HTTP client has to do.

## Development

The development environment is a nix flake; `direnv allow` or `nix develop` provides the toolchain. CI runs these checks:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

The tests replay responses recorded from real servers. `crates/core/tests/fixtures/record.sh` records them again.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
