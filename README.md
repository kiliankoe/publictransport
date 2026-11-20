<!-- ANCHOR: site -->

# 🚉 Public Transport SDK

A client library for public transport APIs, for Rust, Python, Swift and Kotlin. The libraries use a shared model for stops, departures and trips that covers several server types, so an application can query any supported network the same way.

The project is at an early stage. It can search for locations by name, list the departures at a stop and plan trips between two places on servers that speak one of six protocols:

- EFA, the JSON interface of MDV's journey planner, used by many German transport associations
- MOTIS, the routing engine behind [Transitous](https://transitous.org)
- TRIAS 1.2, the German XML standard VDV 431-2
- OJP 1.0, its European successor
- HAFAS mgate, the JSON interface behind the apps of many operators
- OpenTripPlanner 2 with its Transmodel API

The library ships with networks that were checked against their live servers, named by `BundledNetwork` and listed by `Registry::bundled()`: VVO, VRR, Transitous, Entur and the HAFAS servers of about 35 operators in Europe and the USA. Further networks can be described in code or read from entries in the format of [transport-apis](https://github.com/public-transport/transport-apis). Each network says where it has coverage, so an application can pick the networks for a position.

## Design

The core crate does no I/O. It builds HTTP requests and parses HTTP responses, and the caller sends them. This keeps the core usable from other languages and from WebAssembly, where each platform brings its own HTTP stack. The crate documentation of `publictransport` shows the request and parse steps and what the HTTP client has to do.

Rust applications that do not want to bring their own HTTP client use `publictransport-client`, which sends the requests with `reqwest`. Packages for Python, Swift and Kotlin wrap the same core, each with a client on its platform's HTTP stack: httpx, `URLSession` and OkHttp.

The [documentation](https://kiliankoe.github.io/publictransport/) has a guide for each language, and [examples](https://github.com/kiliankoe/publictransport/tree/main/examples) has a short program for each kind of request.

## Install

| Language | Package                                                                                                   |                                                                                                                                                                          |
| -------- | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust     | `cargo add publictransport-client`, or `publictransport` alone to bring your own HTTP client              | [crates/client](https://github.com/kiliankoe/publictransport/tree/main/crates/client), [crates/core](https://github.com/kiliankoe/publictransport/tree/main/crates/core) |
| Python   | `uv add publictransport` or `pip install publictransport`                                                 | [packages/python](https://github.com/kiliankoe/publictransport/tree/main/packages/python)                                                                                |
| Swift    | `.package(url: "https://github.com/kiliankoe/publictransport", from: "0.1.1")`, product `PublicTransport` | [packages/swift](https://github.com/kiliankoe/publictransport/tree/main/packages/swift)                                                                                  |
| Kotlin   | `implementation("io.github.kiliankoe:publictransport:0.1.1")`                                             | [packages/kotlin](https://github.com/kiliankoe/publictransport/tree/main/packages/kotlin)                                                                                |

## Shared Core

Every language runs the same Rust core, so a request is built and an answer is read the same way everywhere.

- The model is defined once, in Rust. Each language gets the types that [UniFFI](https://mozilla.github.io/uniffi-rs/) generates from it. To make things more idiomatic, the generator's settings and a small script adapt details, such as `scripts/swift-idioms.py` and `scripts/kotlin-idioms.py`.
- Written by hand in each language are the client on the platform's HTTP stack, and the bridges to the platform's own types, such as `TimeZone` in Swift.
- A convenience that every language can use goes into the Rust code, not into one package.

<!-- ANCHOR_END: site -->

## Development

Clone with `--recurse-submodules`, or run `git submodule update --init` afterwards: the network data of [transport-apis](https://github.com/public-transport/transport-apis) is a submodule under `crates/core/data`. Networks that repository does not have yet are in `crates/core/data/networks`, in the same format.

The development environment is a nix flake; `direnv allow` or `nix develop` provides the toolchain. The `Justfile` has the commands of the project, and `just` lists them. CI runs these checks, which `just check` runs too:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

The Python package in `packages/python`, the Swift package in `packages/swift` and the Kotlin package in `packages/kotlin` have their own READMEs. The documentation site in `site` is an [mdBook](https://rust-lang.github.io/mdBook/) that includes the READMEs and the examples, so write user documentation there; `just site-serve` shows it, and a release publishes it. The Swift package has its `Package.swift` at the root of this repository, where SwiftPM looks for it. The crate in `bindings/uniffi` builds the library for all three, and `scripts/build-swift.sh` generates the Swift code and builds the XCFramework. The Kotlin code is generated by the Gradle build.

The tests replay responses recorded from real servers. `crates/core/tests/fixtures/record.sh` records them again. `just check-networks` asks every bundled network's live server for stops, departures and trips, to see which still work. `scripts/validate-xml-requests.sh` checks the TRIAS and OJP requests that the library builds against the official schemas, which it fetches from the VDV repositories.

Please also see `CONTRIBUTING.md` for further details.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option. [NOTICE](NOTICE) names the parts that have another license: the code that UniFFI generates and the network data.
