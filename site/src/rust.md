# publictransport for Rust

Two crates make up the Rust library. [`publictransport-client`](https://docs.rs/publictransport-client) sends requests with [reqwest](https://docs.rs/reqwest) and is what most applications want. [`publictransport`](https://docs.rs/publictransport) is the core that it is built on: it builds HTTP requests and parses HTTP responses without doing I/O, for applications that bring their own HTTP client.

```sh
cargo add publictransport-client
```

The client needs a Tokio runtime. It re-exports the core, so an application depends on the one crate.

```rust,ignore
use publictransport_client::{BundledNetwork, Client, DeparturesQuery, LocationKind, LocationQuery, Network};

let client = Client::new("my-app/1.0 (+https://example.org)")?;
let network = Network::bundled(BundledNetwork::VvoEfa);

let query = LocationQuery::new("Hauptbahnhof").with_kinds([LocationKind::Stop]);
let stops = client.locations(&network, query).await?;
let [from, to, ..] = stops.as_slice() else { return Ok(()) };

for departure in client.departures(&network, DeparturesQuery::new(from).with_limit(10)).await? {
    // The time on the clocks at the stop, where its time zone is known.
    if let Some(time) = departure.local_scheduled_time() {
        println!("{time} {} {:?}", departure.line.name, departure.direction);
    }
}
```

## Trips

```rust,ignore
let query = TripsQuery::new(from, to).with_limit(5);
let found = client.trips(&network, &query).await?;
for trip in &found.trips {
    println!("{} seconds, {} changes", trip.duration_seconds(), trip.transfers());
}

// The same query again, with the reference to the trips after these.
if let Some(later) = found.later {
    let more = client.trips(&network, &query.with_page(later)).await?;
}
```

An end of a trip is a `Location`, the ID of a stop as text or a `Coordinate`. `with_departure` and `with_arrival` ask for another time than now.

```rust,ignore
let home = Coordinate { latitude: 51.04, longitude: 13.73 };
let query = TripsQuery::new(home, "de:14612:28").with_arrival(tomorrow_at_nine);
```

## Networks

```rust,ignore
let registry = Registry::bundled();
let network = registry.get("vrr-efa"); // None if no network has this ID

// The networks for a position, the best coverage first.
for covering in registry.covering(Coordinate { latitude: 51.22, longitude: 6.79 }) {
    println!("{:?} {}", covering.tier, covering.network.name);
}
```

The [list of bundled networks](networks.md) also says what a network of your own needs. Describe one with `Network::new` and its `with_` methods, or read one from an entry of [transport-apis](https://github.com/public-transport/transport-apis) with `Network::from_json`. A header for a key or a client name goes on the same way:

```rust,ignore
let entur = Network::bundled(BundledNetwork::Entur).with_header("ET-Client-Name", "mycompany-myapp");
```

## Times, delays and platforms

Times are `Timestamp` values in UTC. A departure and an end of a leg also give them as the clocks at their stop show them, which is what to print.

```rust,ignore
departure.local_scheduled_time(); // None where the time zone of the stop is unknown
departure.local_expected_time();  // the real-time one, if there is one
departure.delay_seconds();        // None without real-time data
departure.platform();             // the real-time platform, otherwise the scheduled one
stop.local_time(any_time);        // the same for any time at a location
```

## Errors

```rust,ignore
use publictransport_client::{ApiError, Error};

match client.departures(&network, "no-such-stop").await {
    Ok(departures) => { /* ... */ }
    Err(Error::Api(ApiError::NotFound { .. })) => { /* the server does not know the stop */ }
    Err(Error::Api(ApiError::RateLimited { retry_after_seconds })) => { /* how long the server asks to wait, if it says so */ }
    Err(Error::Api(error)) => { /* what the server or the query got wrong */ }
    Err(error) => { /* the HTTP exchange failed, or the answer was above 16 MB */ }
}
```

The API documentation on docs.rs has the details: [`publictransport-client`](https://docs.rs/publictransport-client) for the client and its features, and [`publictransport`](https://docs.rs/publictransport) for the model and for the request and parse steps that an HTTP client of your own needs.

## Examples

The repository has these as programs to run, in [`examples`](https://github.com/kiliankoe/publictransport/tree/main/examples).

### Search for places by name

```rust,ignore
{{#include ../../examples/rust/locations.rs}}
```

### List the next departures at a stop

```rust,ignore
{{#include ../../examples/rust/departures.rs}}
```

### Plan trips, and ask for later ones

```rust,ignore
{{#include ../../examples/rust/trips.rs}}
```

### Find the bundled networks for a position

```rust,ignore
{{#include ../../examples/rust/networks.rs}}
```
