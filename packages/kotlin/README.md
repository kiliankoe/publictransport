<!-- ANCHOR: site -->
# publictransport for Kotlin

A Kotlin client for public transport APIs. It uses a shared model for stops, departures and trips that covers several server types, so an application can query any supported network the same way. It supports servers that speak EFA (used by many German transport associations), MOTIS (the routing engine behind [Transitous](https://transitous.org)), TRIAS 1.2, OJP 1.0, HAFAS mgate (the interface behind many operators' apps) and OpenTripPlanner 2 with its Transmodel API.

The package wraps the Rust library of the same name. The Rust code builds the requests and parses the answers, and [OkHttp](https://square.github.io/okhttp/) sends them from suspend functions. It runs on the JVM from Java 11.

The package is not published yet. Until it is on Maven Central, build it from a checkout of the repository, as its [README](https://github.com/kiliankoe/publictransport/tree/main/packages/kotlin#development) describes. An Android library is planned.

```kotlin
import publictransport.BundledNetwork
import publictransport.Client
import publictransport.LocationKind

val client = Client("my-app/1.0 (+https://example.org)")
val network = BundledNetwork.VVO_EFA.network()

val stops = client.locations(network, "Hauptbahnhof", kinds = listOf(LocationKind.STOP))
val (from, to) = stops

for (departure in client.departures(network, from, limit = 10)) {
    // The time on the clocks at the stop, where its time zone is known.
    val time = departure.stop.localTime(departure.scheduledTime)
    println("${time?.toLocalTime() ?: "?"} ${departure.line.name} ${departure.direction ?: ""}")
}

val found = client.trips(network, from, to, limit = 5)
// The same arguments again, with the reference to the trips after these.
val more = client.trips(network, from, to, limit = 5, page = found.later)
```

The [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/kotlin) have a short program for each kind of request.

`BundledNetwork` names the networks that ship with the package, and `network()` gives one. `Registry.bundled()` holds them all, for a list to choose from: `get("vvo-efa")` finds one by its ID as text, and `covering(Coordinate(...))` lists the networks for a position, each with the tier of its coverage there. A `Network` can also be written by hand, as `Network(id = ..., name = ..., endpoint = ..., protocol = Protocol.EFA, timezone = "Europe/Berlin")`, and `networkFromJson(id, json)` reads further ones from entries in the format of [transport-apis](https://github.com/public-transport/transport-apis).

The model types are data classes with `val` properties, so `copy` makes a changed one: `network.copy(headers = network.headers + Header("X-Api-Key", key))`.

`departures` takes a `Location` from a search, a departure or a trip, or the ID of a stop as a `String`. `trips` takes two locations, or two `Place` values: a `Place.Location`, a `Place.Coordinate`, or a `Place.Id` on the network that is asked. A location that another network returned works as well: for a trip always if it has a coordinate, for departures where the asked network can name the stop, which EFA, TRIAS and OJP can for stops with a global ID. Each call also takes a query of the Rust library (`LocationQuery`, `DeparturesQuery`, `TripsQuery`) in place of the plain values.

Times are `java.time.Instant` values. `Location.localTime(time)` gives one as the clocks at that location show it, an `OffsetDateTime` with the offset that holds there at that time, or `null` where no time zone is known for it. An EFA network needs its `timezone` to send a query time. A TRIAS or OJP network needs a `requestorRef`, which public servers document, e.g. `OpenService`. A HAFAS network needs `hafas = HafasOptions(...)` and its `timezone`; the bundled ones have both.

`trips` returns a `Trips` value: the list is in `trips`, and `earlier` and `later` are references for more. Pass one as `page` with the same other arguments to continue the search. An answer from an OpenTripPlanner server can be empty at night and still have a `later` reference to follow.

Failures that the server or the query cause are thrown as subclasses of `PublicTransportException`, such as `PublicTransportException.NotFound` or `PublicTransportException.RateLimited`; its `message` is a sentence in English. Failures of the HTTP exchange are thrown as OkHttp throws them, as an `IOException`. An answer above 16 MB is a `ResponseTooLargeException`, which is an `IOException` too. A cancelled coroutine cancels its request. The client does not follow redirects: a moved server is `PublicTransportException.Moved`. It does not retry, limit its request rate or cache.

One client serves every network, so create one and keep it. It can be built on an `OkHttpClient` of your own, `Client(userAgent, httpClient)`, for example to use a proxy or an interceptor. The client uses its connections and settings, except that it never follows a redirect and never sends a request again.

Some networks need more than their entry holds. Entur asks every client for its name in the header `ET-Client-Name` (`company-application`) and limits clients without it: add it to the headers of the bundled network with `copy`, as above.

An application with its own HTTP stack can skip the client: `Network.locationsRequest(query)` returns the request to send, and `Network.parseLocations(response)` reads the answer. Departures and trips have the same pair.

## Known limitations

The model classes are generated by [UniFFI](https://mozilla.github.io/uniffi-rs/), which shows in a few places.

- The value of a `Place` or a `TripTime` case is named `v1`, as in `Place.Id("de:14612:28").v1`.
- `toString()` of a model value is the debug text of the Rust library, with Rust's names for the fields. It leaves out the values of headers, which can be credentials.
- The package `publictransport` also has the helpers of the generated code, such as `FfiConverterString`. They are no part of the API.
- The documentation of the model types comes from the Rust code, so it links and names things as Rust does: ``[`Location::local_time`]`` is `Location.localTime`, `None` is `null`, and `Error` is `PublicTransportException`.
- A `Registry` is `AutoCloseable`, because it lives in the Rust library. Closing it is optional: its memory is freed when the object is collected.
- The Rust library is loaded with [JNA](https://github.com/java-native-access/jna). From Java 24 on, the JVM prints a warning about that unless it is started with `--enable-native-access=ALL-UNNAMED`.

<!-- ANCHOR_END: site -->

## Development

The package is built with Gradle, in the repository's nix shell (`nix develop`). The build runs `cargo` to build the Rust library in `bindings/uniffi` for this machine, lets UniFFI generate the Kotlin code of the model from it into `build`, and puts the library on the class path, where JNA finds it. `scripts/kotlin-idioms.py` adapts the generated code: it corrects the name of the exception where UniFFI leaves the old one, makes the sentence from Rust the `message` of an exception, and turns methods that only read fields into properties.

```sh
gradle build               # build the Rust library, generate the model, test, compile the examples
gradle :examples:trips     # run an example
```

`just kotlin` runs ktlint and the build from the root of the repository. The examples in `examples/kotlin` are a subproject of this build.

The group ID in `gradle.properties` is a placeholder until the package is published.
