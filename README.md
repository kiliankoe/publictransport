<!-- ANCHOR: site -->

# 🚉 Public Transport SDK

A client library for public transport APIs, for Rust, Python, Swift, Kotlin and TypeScript. The libraries use a shared model for stops, departures and trips that covers several server types, so an application can query any supported network the same way.

The project is at an early stage. It can search for locations by name, find the stops near a position, list the departures and arrivals at a stop and plan trips between two places, on servers that speak one of six protocols:

- EFA, the JSON interface of MDV's journey planner, used by many German transport associations
- MOTIS, the routing engine behind [Transitous](https://transitous.org)
- TRIAS 1.2, the German XML standard VDV 431-2
- OJP 1.0, its European successor
- HAFAS mgate, the JSON interface behind the apps of many operators
- OpenTripPlanner 2 with its Transmodel API

It ships with [networks](https://kiliankoe.github.io/publictransport/networks.html) that were checked against their live servers: VVO, VRR, Transitous, Entur and the HAFAS servers of about 35 operators in Europe and the USA. Each says where it has coverage, so an application can pick the networks for a position. Further networks can be described in code or read from entries in the format of [transport-apis](https://github.com/public-transport/transport-apis).

## Usage

Every language has the same requests. This is Python, and the guides in the [documentation](https://kiliankoe.github.io/publictransport/) show the others.

```python
from publictransport import BundledNetwork, Client, Coordinate, LocationKind

network = BundledNetwork.VVO_EFA.network()

with Client(user_agent="my-app/1.0 (+https://example.org)") as client:
    stops = client.locations(network, "Hauptbahnhof", kinds=[LocationKind.STOP])
    departures = client.departures(network, stops[0], limit=10)
    found = client.trips(network, stops[0], stops[1])
```

`nearby` lists the stops around a position, the nearest first, and `distance_meters` gives the straight-line distance between two positions.

```python
position = Coordinate(latitude=51.04, longitude=13.7325)
for stop in client.nearby(network, position, radius_meters=500):
    if stop.coordinate:
        print(stop.name, round(position.distance_meters(stop.coordinate)), "m")
```

[examples](https://github.com/kiliankoe/publictransport/tree/main/examples) has a short program for each kind of request in each language.

## Install

| Language   | Package                                                                                                   |                                                                                                                                                                          |
| ---------- | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust       | `cargo add publictransport-client`, or `publictransport` alone to bring your own HTTP client              | [crates/client](https://github.com/kiliankoe/publictransport/tree/main/crates/client), [crates/core](https://github.com/kiliankoe/publictransport/tree/main/crates/core) |
| Python     | `uv add publictransport` or `pip install publictransport`                                                 | [packages/python](https://github.com/kiliankoe/publictransport/tree/main/packages/python)                                                                                |
| Swift      | `.package(url: "https://github.com/kiliankoe/publictransport", from: "0.1.2")`, product `PublicTransport` | [packages/swift](https://github.com/kiliankoe/publictransport/tree/main/packages/swift)                                                                                  |
| Kotlin     | `implementation("io.github.kiliankoe:publictransport:0.1.2")`                                             | [packages/kotlin](https://github.com/kiliankoe/publictransport/tree/main/packages/kotlin)                                                                                |
| TypeScript | `pnpm add @kiliankoe/publictransport` or `npm install @kiliankoe/publictransport`                         | [packages/typescript](https://github.com/kiliankoe/publictransport/tree/main/packages/typescript)                                                                        |

## Design

Every language runs the same Rust core, so a request is built and an answer is read the same way everywhere.

- The core does no I/O. It builds HTTP requests and parses HTTP responses, and the caller sends them, so each platform brings its own HTTP stack: reqwest in `publictransport-client`, httpx, `URLSession`, OkHttp and `fetch`.
- The model is defined once, in Rust. Python, Swift and Kotlin get the types that [UniFFI](https://mozilla.github.io/uniffi-rs/) generates from it, adapted by the generator's settings and a small script each. TypeScript runs the core as WebAssembly and gets its types from [wasm-bindgen](https://wasm-bindgen.github.io/wasm-bindgen/) and [tsify](https://github.com/madonoharu/tsify).
- Written by hand in each language are the client and the bridges to the platform's own types, such as `TimeZone` in Swift or the error classes in TypeScript.
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

The Python package in `packages/python`, the Swift package in `packages/swift`, the Kotlin package in `packages/kotlin` and the TypeScript package in `packages/typescript` have their own READMEs. The documentation site in `site` is an [mdBook](https://rust-lang.github.io/mdBook/) that includes the READMEs and the examples, so write user documentation there; `just site-serve` shows it, and a release publishes it. The Swift package has its `Package.swift` at the root of this repository, where SwiftPM looks for it. The crate in `bindings/uniffi` builds the library for Python, Swift and Kotlin, and `scripts/build-swift.sh` generates the Swift code and builds the XCFramework. The Kotlin code is generated by the Gradle build. The crate in `bindings/wasm` is the WebAssembly module of the TypeScript package, which `packages/typescript/build.sh` builds.

The tests replay responses recorded from real servers. `crates/core/tests/fixtures/record.sh` records them again. `just check-networks` asks every bundled network's live server for stops, departures and trips, to see which still work. `scripts/validate-xml-requests.sh` checks the TRIAS and OJP requests that the library builds against the official schemas, which it fetches from the VDV repositories.

Please also see `CONTRIBUTING.md` for further details.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option. [NOTICE](NOTICE) names the parts that have another license: the code that UniFFI generates and the network data.
