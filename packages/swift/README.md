<!-- ANCHOR: site -->
# PublicTransport for Swift

A Swift client for public transport APIs. One model for stops, departures and trips covers servers that speak EFA, MOTIS, TRIAS 1.2, OJP 1.0, HAFAS mgate or OpenTripPlanner 2, so an application queries every supported network the same way.

```swift
.package(url: "https://github.com/kiliankoe/publictransport", from: "0.1.2")
```

It runs on iOS, macOS, tvOS, watchOS and visionOS from version 26, and on Linux. The package wraps the Rust library of the same name, which builds the requests and parses the answers, and `URLSession` sends them.

```swift
import PublicTransport

let client = Client(userAgent: "my-app/1.0 (+https://example.org)")
let network = Network.bundled(.vvoEfa)

let stops = try await client.locations(in: network, matching: "Hauptbahnhof", kinds: [.stop])
guard let from = stops.first, let to = stops.dropFirst().first else { return }

for departure in try await client.departures(in: network, at: from, limit: 10) {
    print(departure.scheduledTime, departure.line.name, departure.direction ?? "")
}
```

`nearby` is the second way to find stops: it lists the stops around a position, the nearest first. `distanceMeters(to:)` gives the straight-line distance between two positions in meters. The position is a `Coordinate` or a `CLLocationCoordinate2D`.

```swift
let position = Coordinate(latitude: 51.04, longitude: 13.7325)
for stop in try await client.nearby(in: network, at: position, radiusMeters: 500) {
    guard let coordinate = stop.coordinate else { continue }
    print(stop.name, Int(position.distanceMeters(to: coordinate)), "m")
}
```

`arrivals` takes the same arguments and lists what arrives at the stop. An arrival has an `origin`, where the vehicle comes from, in place of the `direction`.

```swift
let arrivals = try await client.arrivals(in: network, at: from, limit: 10)
```

`journey` gives the run of one vehicle: every stop it calls at, with times and platforms. It takes the `journeyId` of a departure, an arrival or a leg of a trip. The ID only works on the network it came from, and for about a day.

```swift
let departures = try await client.departures(in: network, at: from, limit: 10)
if let id = departures.compactMap(\.journeyId).first {
    for stopover in try await client.journey(in: network, id: id).stops {
        guard let time = stopover.scheduledDeparture ?? stopover.scheduledArrival else { continue }
        print(time, stopover.stop.name)
    }
}
```

The [documentation](https://kiliankoe.github.io/publictransport/swift.html) has a program for each kind of request, which are also in [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/swift).

## Trips

```swift
let found = try await client.trips(in: network, from: from, to: to, limit: 5)
for trip in found.trips {
    print(trip.durationSeconds, "seconds,", trip.transfers, "changes")
}

// The same arguments again, with the reference to the trips after these.
let more = try await client.trips(in: network, from: from, to: to, limit: 5, page: found.later)
```

An end of a trip is a `Location`, the ID of a stop as a `String`, a `Coordinate` or a `CLLocationCoordinate2D`. `departingAt:` and `arrivingBy:` ask for another time than now.

```swift
let home = Coordinate(latitude: 51.04, longitude: 13.73)
try await client.trips(in: network, from: home, to: "de:14612:28", arrivingBy: tomorrowAtNine)
```

## Networks

```swift
let registry = Registry.bundled()
let network = registry.network(id: "vrr-efa")  // nil if no network has this ID

// The networks for a position, the best coverage first.
for covering in registry.covering(at: Coordinate(latitude: 51.22, longitude: 6.79)) {
    print(covering.tier, covering.network.name)
}
```

The [list of bundled networks](https://kiliankoe.github.io/publictransport/networks.html) also says what a network of your own needs. Describe one in code, or read one from an entry of [transport-apis](https://github.com/public-transport/transport-apis) with `Network(id:json:)`. A header for a key or a client name is one more entry in its list:

```swift
var network = Network.bundled(.entur)
network.headers.append(Header(name: "ET-Client-Name", value: "mycompany-myapp"))
```

## Times, delays and platforms

Times are `Date` values, and a location has the time zone to show them in.

```swift
departure.expectedTime    // the real-time one, if there is one
departure.stop.timeZone   // nil where the server does not tell it
departure.delaySeconds    // nil without real-time data
departure.platform        // the real-time platform, otherwise the scheduled one
```

An end of a leg has the same properties.

## Disruptions and hints

A departure, a leg of a trip and the run of a vehicle have `remarks`: the texts that the server sends with them. Every protocol but MOTIS fills them. A remark is one of two kinds. A disruption is a message from the incident system of the operator, such as a closed road or a broken elevator. A hint is a fixed property of the ride, such as air conditioning or a note for wheelchair users.

```swift
for remark in departure.remarks where remark.kind == .disruption {
    print(remark.title ?? remark.text ?? "")
    print(remark.url ?? "")  // a page with more about it, or nil
    print(remark.validFrom, remark.validUntil)  // nil where the server does not say
}
```

At least one of `title` and `text` is set. `text` is plain text without HTML, in the language of the server. The server sends a disruption with every departure and leg it concerns, so an application that shows each message once tells them apart by `id`. A hint has no `id`. It has the `code` that the server gives its kind, such as `A__KL` for air conditioning on German EFA servers, for an application that shows icons for the ones it knows. Operators define these codes, so they differ between networks.

## SwiftUI and storing values

`Location` and `Network` are `Identifiable`. A `Departure` and a `Trip` have no identifier from the server; they are `Hashable`, so a list can use `id: \.self`.

What a query takes and what an answer holds is `Codable`. The stored form follows the model, so it can change with any minor version before 1.0: read stored values with `try?` and fetch them again when that fails. For what has to outlive an update, such as a favorite stop, store `network.id` and `location.id`. A `Network` is not `Codable`, because it can hold credentials; find it again with `Registry.bundled().network(id:)`.

## Errors

```swift
do {
    _ = try await client.departures(in: network, at: "no-such-stop")
} catch let error as PublicTransportError {
    print(error.localizedDescription)  // a sentence in English
}
```

What the server or the query cause is a `PublicTransportError`, such as `.notFound` or `.rateLimited(retryAfterSeconds:)`, which holds how long the server asks to wait, if it says so. A failed HTTP exchange is a `URLError`, a cancelled task throws `CancellationError`, and an answer above 16 MB is `ClientError.responseTooLarge`.

## Known limitations

- On Apple platforms the package downloads the Rust library as a binary from the release. The code on a branch can be ahead of that binary, so a dependency on a branch does not work.
- On Linux the package links the library from a directory you name, see "Development".
- On watchOS the library covers only the watches with a 64-bit processor (Series 9 and later), so an app for all watches does not link yet.

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
