<!-- ANCHOR: site -->
# publictransport for Kotlin

A Kotlin client for public transport APIs. One model for stops, departures and trips covers servers that speak EFA, MOTIS, TRIAS 1.2, OJP 1.0, HAFAS mgate or OpenTripPlanner 2, so an application queries every supported network the same way.

```kotlin
dependencies {
    implementation("io.github.kiliankoe:publictransport:0.2.0")
}
```

It runs on the JVM from Java 11, on macOS and Linux (with glibc, so not Alpine) on x86-64 and ARM and on Windows on x86-64. An Android app takes `io.github.kiliankoe:publictransport-android` instead, which is the same code for Android 8.0 (API level 26) and later, see [Android](#android). The package wraps the Rust library of the same name, which builds the requests and parses the answers, and [OkHttp](https://square.github.io/okhttp/) sends them from suspend functions.

```kotlin
import publictransport.BundledNetwork
import publictransport.Client
import publictransport.Coordinate
import publictransport.LocationKind

val client = Client("my-app/1.0 (+https://example.org)")
val network = BundledNetwork.VVO_EFA.network()

val stops = client.locations(network, "Hauptbahnhof", kinds = listOf(LocationKind.STOP))
val (from, to) = stops

for (departure in client.departures(network, from, limit = 10)) {
    println("${departure.scheduledTime} ${departure.line.name} ${departure.direction ?: ""}")
}
```

`nearby` is the second way to find stops: it lists the stops around a position, the nearest first. `Coordinate.distanceMeters` gives the straight-line distance between two positions in meters.

```kotlin
val position = Coordinate(51.04, 13.7325)
for (stop in client.nearby(network, position, radiusMeters = 500)) {
    val distance = stop.coordinate?.let(position::distanceMeters) ?: continue
    println("${stop.name} ${distance.toInt()} m")
}
```

`arrivals` takes the same arguments and lists what arrives at the stop. An arrival has an `origin`, where the vehicle comes from, in place of the `direction`.

```kotlin
val arrivals = client.arrivals(network, from, limit = 10)
```

`journey` gives the run of one vehicle: every stop it calls at, with times and platforms. It takes the `journeyId` of a departure, an arrival or a leg of a trip. The ID only works on the network it came from, and for about a day. A stopover gives each of its sides with `departure` and `arrival` as an end of a leg, which has the delay and the local time, and `platform` works on it as on a departure.

```kotlin
val id = client.departures(network, from, limit = 10).firstNotNullOfOrNull { it.journeyId }
if (id != null) {
    for (stopover in client.journey(network, id).stops) {
        // The last stop has no departure.
        val end = stopover.departure ?: stopover.arrival
        println("${end?.localExpectedTime} ${stopover.stop.name}")
    }
}
```

One client serves every network, so create one and keep it. The [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/kotlin) have a program for each kind of request.

## Trips

```kotlin
val found = client.trips(network, from, to, limit = 5)
for (trip in found.trips) {
    println("${trip.durationSeconds} seconds, ${trip.transfers} changes")
}

// The same arguments again, with the reference to the trips after these.
val more = client.trips(network, from, to, limit = 5, page = found.later)
```

`trips` takes two locations, or two `Place` values for other ends: `asPlace()` makes one from a location or a `Coordinate`, and `Place.Id` names a stop by its ID. `departAt` and `arriveBy` ask for another time than now.

```kotlin
val home = Coordinate(51.04, 13.73).asPlace()
client.trips(network, home, Place.Id("de:14612:28"), arriveBy = tomorrowAtNine)
```

## Android

```kotlin
dependencies {
    implementation("io.github.kiliankoe:publictransport-android:0.2.0")
}
```

The Android library has the same classes in the same package, with the Rust library for `arm64-v8a`, `armeabi-v7a` and `x86_64`. It brings the rules that R8 needs, and OkHttp brings the permission to use the internet. A call can be made from the main thread, for example in `viewModelScope`: it suspends, and the answer is read and parsed on a thread of OkHttp.

## Networks

```kotlin
val registry = Registry.bundled()
val network = registry.get("vrr-efa") // null if no network has this ID

// The networks for a position, the best coverage first.
for (covering in registry.covering(Coordinate(51.22, 6.79))) {
    println("${covering.tier} ${covering.network.name}")
}
```

The [list of bundled networks](https://kiliankoe.github.io/publictransport/networks.html) also says what a network of your own needs. Write one as `Network(id = ..., name = ..., endpoint = ..., protocol = Protocol.EFA, timezone = "Europe/Berlin")`, or read one from an entry of [transport-apis](https://github.com/public-transport/transport-apis) with `networkFromJson(id, json)`. The model types are data classes, so `copy` adds a header for a key or a client name:

```kotlin
val entur = BundledNetwork.ENTUR.network()
val named = entur.copy(headers = entur.headers + Header("ET-Client-Name", "mycompany-myapp"))
```

## Times, delays and platforms

Times are `java.time.Instant` values. A departure, an arrival and an end of a leg also give them as the clocks at their stop show them, as an `OffsetDateTime`, which is what to print.

```kotlin
departure.localScheduledTime // null where the time zone of the stop is unknown
departure.localExpectedTime  // the real-time one, if there is one
departure.delaySeconds       // null without real-time data
departure.platform           // the real-time platform, otherwise the scheduled one
stop.localTime(anyTime)      // the same for any time at a location
```

## Disruptions and hints

A departure, a leg of a trip and the run of a vehicle have `remarks`: the texts that the server sends with them. Every protocol but MOTIS fills them. A remark is one of two kinds. A disruption is a message from the incident system of the operator, such as a closed road or a broken elevator. A hint is a fixed property of the ride, such as air conditioning or a note for wheelchair users.

```kotlin
for (remark in departure.remarks.filter { it.kind == RemarkKind.DISRUPTION }) {
    println(remark.title ?: remark.text)
    println(remark.url) // a page with more about it, or null
    println("${remark.validFrom} ${remark.validUntil}") // null where the server does not say
}
```

At least one of `title` and `text` is set. `text` is plain text without HTML, in the language the server answers in. The `language` of the network asks for one, and the [list of networks](https://kiliankoe.github.io/publictransport/networks.html) says what servers make of it. The server sends a disruption with every departure and leg it concerns, so an application that shows each message once tells them apart by `id`. A hint has no `id`. It has the `code` that the server gives its kind, such as `A__KL` for air conditioning on German EFA servers, for an application that shows icons for the ones it knows. Operators define these codes, so they differ between networks.

## Errors

```kotlin
try {
    client.departures(network, "no-such-stop")
} catch (error: PublicTransportException.NotFound) {
    // ...
} catch (error: PublicTransportException.RateLimited) {
    println(error.retryAfterSeconds) // how long the server asks to wait, or null
} catch (error: PublicTransportException) {
    println(error.message) // a sentence in English
}
```

What the server or the query cause is a subclass of `PublicTransportException`. A failed HTTP exchange is an `IOException` from OkHttp, and so is an answer above 16 MB, as a `ResponseTooLargeException`. A cancelled coroutine cancels its request.

## Your own HTTP client

`Client(userAgent, httpClient)` builds on an `OkHttpClient` of yours, for example to use a proxy or an interceptor. The client uses its connections and settings, except that it never follows a redirect and never sends a request again.

An application with another HTTP stack skips the client. The network builds each request and reads its answer, and the same pair exists for nearby stops, departures, arrivals and trips:

```kotlin
val request = network.locationsRequest(query) // a LocationQuery
val locations = network.parseLocations(response)
```

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

The Android library in `android` is a part of this build in the shell that has the Android SDK, `nix develop .#android`. Entering that shell accepts the license of the SDK. There `just android` builds and lints the library and builds an app with it that R8 has shrunk and renamed, and `just android-test` runs the test of that app on a connected device or a running emulator. The test shows what the tests on the JVM cannot: that the app finds the Rust library and that it still works after R8.

A release publishes the package to Maven Central with the Rust library of every platform in the jar, and the Android library next to it. The release workflow takes these libraries from the wheels of the Python package and passes their directory as `-PnativeLibraries`.
