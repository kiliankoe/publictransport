# publictransport for Rust

Two crates make up the Rust library. [`publictransport-client`](https://docs.rs/publictransport-client) sends requests with [reqwest](https://docs.rs/reqwest) and is what most applications want. [`publictransport`](https://docs.rs/publictransport) is the core that it is built on: it builds HTTP requests and parses HTTP responses without doing I/O, for applications that bring their own HTTP client.

```sh
cargo add publictransport-client
```

The client needs a Tokio runtime. It re-exports the core, so an application depends on the one crate.

```rust,ignore
use publictransport_client::{BundledNetwork, Client, LocationKind, LocationQuery, Network, TripsQuery};

let client = Client::new("my-app/1.0 (+https://example.org)")?;
let vvo = Network::bundled(BundledNetwork::VvoEfa);

let query = LocationQuery::new("Hauptbahnhof").with_kinds([LocationKind::Stop]);
let stops = client.locations(&vvo, query).await?;
if let [from, to, ..] = stops.as_slice() {
    for departure in client.departures(&vvo, from).await? {
        // The time on the clocks at the stop, where its time zone is known.
        if let Some(time) = departure.stop.local_time(departure.scheduled_time) {
            println!("{time} {} to {:?}", departure.line.name, departure.direction);
        }
    }
    let found = client.trips(&vvo, TripsQuery::new(from, to)).await?;
}
```

`Network::bundled` gives a network that ships with the crate by name. `Registry::bundled()` holds them all: `get("vvo-efa")` finds one by its ID as text, and `covering(coordinate)` lists the networks for a position. A `Network` can also be described in code with `Network::new`, or read from an entry in the format of [transport-apis](https://github.com/public-transport/transport-apis) with `Network::from_json`.

A stop or a place in a query is a `Location` from a search, a departure or a trip, or an ID as text; a place in a trips query can be a `Coordinate` too. A location that another network returned works as well.

Times are `Timestamp` values in UTC. `Location::local_time` gives one as the clocks at that location show it, or `None` where no time zone is known for it.

An answer to a trips query has the list in `trips`, and `earlier` and `later` are references for more: pass one to `TripsQuery::with_page` on the same query. The `limit` of a query is not a maximum, servers treat it as a guide.

The client does not retry, limit its request rate or cache, and it does not follow redirects. Its `Error` tells a failed HTTP exchange from what the server or the query got wrong.

The API documentation on docs.rs has the details: [`publictransport-client`](https://docs.rs/publictransport-client) for the client and its features, and [`publictransport`](https://docs.rs/publictransport) for the model and for the request and parse steps an own HTTP client needs.

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
