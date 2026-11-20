# Examples

A few usage examples of how this SDK works.

| Example      | What it shows                                                       | Server                           |
| ------------ | ------------------------------------------------------------------- | -------------------------------- |
| `locations`  | Search for places by name                                           | VVO in Dresden, EFA              |
| `departures` | List the next departures at a stop                                  | VBB in Berlin, HAFAS             |
| `trips`      | Plan trips between two stops, and ask for the trips after the first | Entur in Norway, OpenTripPlanner |
| `networks`   | List the bundled networks that cover a position, without a request  | none                             |

## Rust

```sh
cargo run -p publictransport-examples --bin trips
```

## Python

In `packages/python`, after `uv sync` has built the package:

```sh
uv run python ../../examples/python/trips.py
```

With a release installed, `python trips.py` is enough.

## Swift

The package in `examples/swift` depends on the package of this checkout. That one needs the Rust library, which `just swift-library` builds, see [Development](../packages/swift/README.md#development). Outside the development shell:

```sh
PUBLICTRANSPORT_LOCAL_BINARY=1 swift run --package-path examples/swift trips
```

Without the variable it uses the Rust library of the last release, which fits as long as the Swift code has not changed since.

## Kotlin

The programs in `examples/kotlin` are a subproject of the Gradle build in `packages/kotlin`, which builds the Rust library first. In the development shell:

```sh
gradle --project-dir packages/kotlin :examples:trips
```

## TypeScript

The programs in `examples/typescript` use the package in `packages/typescript` through a link, so build that first, see [Development](../packages/typescript/README.md#development), and run `pnpm install` in `examples/typescript`. Node.js runs them as they are, and so do Deno and Bun:

```sh
node examples/typescript/trips.ts
```

`just examples` runs all of them, and `just examples-rust`, `just examples-python`, `just examples-swift`, `just examples-kotlin` and `just examples-typescript` those of one language.
