# PublicTransport for Swift

A Swift client for public transport APIs. One shared model for stops, departures and trips covers several server types, so an application can query any supported network the same way. It supports servers that speak EFA (used by many German transport associations), MOTIS (the routing engine behind [Transitous](https://transitous.org)), TRIAS 1.2, OJP 1.0, HAFAS mgate (the interface behind many operators' apps) and OpenTripPlanner 2 with its Transmodel API.

The package wraps the Rust library of the same name. The Rust code builds the requests and parses the answers, and `URLSession` sends them. It runs on iOS, macOS, tvOS, watchOS and visionOS from version 26, and on Linux.

The package is not released yet, so it cannot be added to a project by its URL: the Rust library it links is not in the repository. Until the first release, build that library as described under "Development" and depend on a checkout by path. On watchOS the library covers only the watches with a 64-bit processor (Series 9 and later), so an app for all watches does not link yet.

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

`Network.bundled(_:)` gives a network that ships with the package by name. `Registry.bundled()` holds them all, for a list to choose from: `network(id:)` finds one by its ID as text, and `covering(at:)` lists the networks for a position. A `Network` can also be described in code or read from an entry in the format of [transport-apis](https://github.com/public-transport/transport-apis) with `Network(id:json:)`.

A stop or a place is a `Location` from a search, a departure or a trip, or a `String` with an ID; a place in `trips` can be a `Coordinate` too, or a `CLLocationCoordinate2D` where Core Location exists. A location that another network returned works as well: for a trip always if it has a coordinate, for departures where the asked network can name the stop, which EFA, TRIAS and OJP can for stops with a global ID. Each call also takes a query of the Rust library (`LocationQuery`, `DeparturesQuery`, `TripsQuery`) with the label `query:`.

Times are `Date` values. `Location.timeZone` is the time zone to show them in.

`Location` and `Network` are `Identifiable`. A `Departure` and a `Trip` have no identifier from the server; they are `Hashable`, so a list can use `id: \.self`. Nothing is `Codable` yet: to keep a favorite, store `network.id` and `location.id`, and find the network again with `Registry.bundled().network(id:)`.

Failures that the server or the query cause are thrown as `PublicTransportError`, such as `.notFound` or `.rateLimited`; its `localizedDescription` is a sentence in English. Failures of the HTTP exchange are thrown as `URLError`, and a cancelled task throws `CancellationError`. An answer above 16 MB is `ClientError.responseTooLarge`. The client does not follow redirects: a moved server is `PublicTransportError.moved`. It does not retry, limit its request rate or cache.

Some networks need more than their entry holds. Entur asks every client for its name in the header `ET-Client-Name` (`company-application`): add it to `network.headers`. An answer to `trips` from an OpenTripPlanner server can be empty at night and still have a `later` reference to follow.

## Development

The Swift code in `Sources/PublicTransport/Generated` and the C header in `Sources/publictransportFFI` are generated from the Rust library and committed, so that the package builds without a Rust toolchain. UniFFI generates the code, and `scripts/swift-idioms.py` then makes it read like Swift: properties where UniFFI writes methods without arguments, error cases in lower camel case, `CaseIterable` enums, and Swift's own text when a value is printed. `scripts/build-swift.sh bindings` writes them again after a change to the Rust library, and CI fails if they are not current.

The Rust library itself is not in the repository. `scripts/build-swift.sh xcframework macos` builds it for macOS, which is enough for `swift test`, and `scripts/build-swift.sh xcframework` for every platform. Both run in the development shell (`nix develop`). Swift itself has to run outside that shell, because the shell sets an SDK that Xcode's Swift cannot use:

```sh
nix develop -c scripts/build-swift.sh xcframework macos
env -i PATH=/usr/bin:/bin HOME=$HOME swift test
```

On Linux the package links the static library, whose directory the linker gets on the command line:

```sh
cargo rustc -p publictransport-uniffi --lib --release --crate-type staticlib
swift test -Xlinker -Ltarget/release
```
