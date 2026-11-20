<!-- ANCHOR: site -->
# PublicTransport for Swift

A Swift client for public transport APIs. It uses a shared model for stops, departures and trips that covers several server types, so an application can query any supported network the same way. It supports servers that speak EFA (used by many German transport associations), MOTIS (the routing engine behind [Transitous](https://transitous.org)), TRIAS 1.2, OJP 1.0, HAFAS mgate (the interface behind many operators' apps) and OpenTripPlanner 2 with its Transmodel API.

The package wraps the Rust library of the same name. The Rust code builds the requests and parses the answers, and `URLSession` sends them. It runs on iOS, macOS, tvOS, watchOS and visionOS from version 26, and on Linux.

Add it to a project by the URL of this repository, and depend on a version:

```swift
.package(url: "https://github.com/kiliankoe/publictransport", from: "0.1.2")
```

On Apple platforms the package downloads the Rust library as a binary from the release. Between releases the code on a branch can be ahead of that binary, so a dependency on a branch does not work. On Linux the package links the library from a directory you name, see "Development". On watchOS the library covers only the watches with a 64-bit processor (Series 9 and later), so an app for all watches does not link yet.

```swift
import PublicTransport

let client = Client(userAgent: "my-app/1.0 (+https://example.org)")
let network = Network.bundled(.vvoEfa)

let stops = try await client.locations(in: network, matching: "Hauptbahnhof", kinds: [.stop])
guard let from = stops.first, let to = stops.dropFirst().first else { return }

for departure in try await client.departures(in: network, at: from, limit: 10) {
    print(departure.scheduledTime, departure.line.name, departure.direction ?? "")
}

let found = try await client.trips(in: network, from: from, to: to, limit: 5)
// The same arguments again, with the reference to the trips after these.
let more = try await client.trips(in: network, from: from, to: to, limit: 5, page: found.later)
```

The [documentation](https://kiliankoe.github.io/publictransport/swift.html) has this guide with a short program for each kind of request, which are also in [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/swift).

`Network.bundled(_:)` gives a network that ships with the package by name. `Registry.bundled()` holds them all, for a list to choose from: `network(id:)` finds one by its ID as text, and `covering(at:)` lists the networks for a position. A `Network` can also be described in code or read from an entry in the format of [transport-apis](https://github.com/public-transport/transport-apis) with `Network(id:json:)`.

A stop or a place is a `Location` from a search, a departure or a trip, or a `String` with an ID; a place in `trips` can be a `Coordinate` too, or a `CLLocationCoordinate2D` where Core Location exists. A location that another network returned works as well: for a trip always if it has a coordinate, for departures where the asked network can name the stop, which EFA, TRIAS and OJP can for stops with a global ID. Each call also takes a query of the Rust library (`LocationQuery`, `DeparturesQuery`, `TripsQuery`) with the label `query:`.

Times are `Date` values. `Location.timeZone` is the time zone to show them in.

`Location` and `Network` are `Identifiable`. A `Departure` and a `Trip` have no identifier from the server; they are `Hashable`, so a list can use `id: \.self`. 

What a query takes and what an answer holds is `Codable`: `Location`, `Departure`, `Trip`, the queries and the types inside them. An enum without values is stored as the name of its case, and a `BundledNetwork` as the ID of its network. An enum with values has the form that Swift writes for it, such as `{"id": {"_0": "de:14612:28"}}` for `Place.id`, and a `Date` the form that the encoder is set to. A `Network` is not `Codable`, because it can hold credentials and has a JSON form of its own; store its ID and find it again with `Registry.bundled().network(id:)`.

The stored form follows the model, so it can change where the model does: with any minor version before 1.0, and with a major version after. Read stored values with `try?` and fetch them again when that fails. For what has to outlive an update, such as a favorite stop, store `network.id` and `location.id`.

Failures that the server or the query cause are thrown as `PublicTransportError`, such as `.notFound` or `.rateLimited`; its `localizedDescription` is a sentence in English. Failures of the HTTP exchange are thrown as `URLError`, and a cancelled task throws `CancellationError`. An answer above 16 MB is `ClientError.responseTooLarge`. The client does not follow redirects: a moved server is `PublicTransportError.moved`. It does not retry, limit its request rate or cache.

Some networks need more than their entry holds. Entur asks every client for its name in the header `ET-Client-Name` (`company-application`): add it to `network.headers`. An answer to `trips` from an OpenTripPlanner server can be empty at night and still have a `later` reference to follow.

<!-- ANCHOR_END: site -->

## Development

The Swift code in `Sources/PublicTransport/Generated` and the C header in `Sources/publictransportFFI` are generated from the Rust library and committed, so that the package builds without a Rust toolchain. UniFFI generates the code, and `scripts/swift-idioms.py` then makes it read like Swift: properties where UniFFI writes methods without arguments, error cases in lower camel case, `CaseIterable` and `Codable` types, Swift's own text when a value is printed, and UniFFI's helpers kept out of the public names. `just swift-bindings` writes them again after a change to the Rust library, and CI fails if they are not current.

The Rust library itself is not in the repository. In the development shell (`nix develop`), `just swift-library` builds it for macOS, which is enough for the tests, and `just swift-library all` for every platform. `just swift-test` runs the tests and the linter against it:

```sh
just swift-library
just swift-test
```

`Package.swift` uses a library built this way when `PUBLICTRANSPORT_LOCAL_BINARY` is set in the environment, and the binary of the last release otherwise. To work on the package in Xcode, start it with the variable set: `PUBLICTRANSPORT_LOCAL_BINARY=1 open -a Xcode .` Swift has to run outside the development shell, because the shell sets an SDK that Xcode's Swift cannot use; `just swift-test` takes care of that.

On Linux the package links the static library, whose directory the linker gets on the command line:

```sh
cargo rustc -p publictransport-uniffi --lib --release --crate-type staticlib
swift test -Xlinker -Ltarget/release
```
