<!-- ANCHOR: site -->
# publictransport

A Python client for public transport APIs. One model for stops, departures and trips covers servers that speak EFA, MOTIS, TRIAS 1.2, OJP 1.0, HAFAS mgate or OpenTripPlanner 2, so an application queries every supported network the same way.

```sh
uv add publictransport    # or: pip install publictransport
```

There are wheels for Linux (glibc), macOS and Windows on x86-64, and for the first two on ARM. The package wraps the Rust library of the same name, which builds the requests and parses the answers, and [httpx](https://www.python-httpx.org) sends them.

```python
from publictransport import BundledNetwork, Client, Coordinate, LocationKind

network = BundledNetwork.VVO_EFA.network()

with Client(user_agent="my-app/1.0 (+https://example.org)") as client:
    stops = client.locations(network, "Hauptbahnhof", kinds=[LocationKind.STOP])
    for departure in client.departures(network, stops[0], limit=10):
        print(departure.scheduled_time, departure.line.name, departure.direction)
```

`nearby` is the second way to find stops: it lists the stops around a position, the nearest first. `Coordinate.distance_meters` gives the straight-line distance between two positions in meters.

```python
position = Coordinate(latitude=51.04, longitude=13.7325)
for stop in client.nearby(network, position, radius_meters=500):
    if stop.coordinate:
        print(stop.name, round(position.distance_meters(stop.coordinate)), "m")
```

`arrivals` takes the same arguments and lists what arrives at the stop. An arrival has an `origin`, where the vehicle comes from, in place of the `direction`.

```python
arrivals = client.arrivals(network, stops[0], limit=10)
```

`AsyncClient` has the same methods as coroutines. The [documentation](https://kiliankoe.github.io/publictransport/python.html) has a program for each kind of request, which are also in [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/python).

## Trips

```python
found = client.trips(network, stops[0], stops[1], limit=5)
for trip in found.trips:
    print(trip.duration_seconds(), "seconds,", trip.transfers(), "changes")

# The same arguments again, with the reference to the trips after these.
more = client.trips(network, stops[0], stops[1], limit=5, page=found.later)
```

An end of a trip is a `Location`, the ID of a stop as text or a `Coordinate`. `depart_at=` and `arrive_by=` ask for another time than now, as a timezone-aware `datetime`.

```python
home = Coordinate(latitude=51.04, longitude=13.73)
client.trips(network, home, "de:14612:28", arrive_by=tomorrow_at_nine)
```

## Networks

```python
from publictransport import Coordinate, Registry

registry = Registry.bundled()
network = registry.get("vrr-efa")  # None if no network has this ID

# The networks for a position, the best coverage first.
for covering in registry.covering(Coordinate(latitude=51.22, longitude=6.79)):
    print(covering.tier, covering.network.name)
```

The [list of bundled networks](https://kiliankoe.github.io/publictransport/networks.html) also says what a network of your own needs. Write one as `Network(id=..., name=..., endpoint=..., protocol=Protocol.EFA, timezone="Europe/Berlin")`, or read one from an entry of [transport-apis](https://github.com/public-transport/transport-apis) with `network_from_json(id, json)`. A header for a key or a client name is one more entry in its list:

```python
network.headers.append(Header(name="ET-Client-Name", value="mycompany-myapp"))
```

## Times, delays and platforms

Times are timezone-aware `datetime` objects in UTC. A departure, an arrival and an end of a leg also give them as the clocks at their stop show them, which is what to print.

```python
departure.local_scheduled_time()  # None where the time zone of the stop is unknown
departure.local_expected_time()  # the real-time one, if there is one
departure.delay_seconds()  # None without real-time data
departure.platform()  # the real-time platform, otherwise the scheduled one
stop.local_time(any_time)  # the same for any time at a location
```

## Disruptions and hints

A departure and a leg of a trip have `remarks`: the texts that the server sends with them. Every protocol but MOTIS fills them. A remark is one of two kinds. A disruption is a message from the incident system of the operator, such as a closed road or a broken elevator. A hint is a fixed property of the ride, such as air conditioning or a note for wheelchair users.

```python
for remark in departure.remarks:
    if remark.kind == RemarkKind.DISRUPTION:
        print(remark.title or remark.text)
        print(remark.url)  # a page with more about it, or None
        print(remark.valid_until)  # None where the server does not say
```

At least one of `title` and `text` is set. `text` is plain text without HTML, in the language of the server. The server sends a disruption with every departure and leg it concerns, so an application that shows each message once tells them apart by `id`. A hint has no `id`. It has the `code` that the server gives its kind, such as `A__KL` for air conditioning on German EFA servers, for an application that shows icons for the ones it knows. Operators define these codes, so they differ between networks.

## Errors

```python
from publictransport import Error

try:
    client.departures(network, "no-such-stop")
except Error.NotFound:
    ...
except Error.RateLimited as error:
    print(error.retry_after_seconds)  # how long the server asks to wait, or None
```

What the server or the query cause is a subclass of `publictransport.Error`. A failed HTTP exchange is raised as httpx raises it, and an answer above 16 MB raises `ResponseTooLarge`.

## Your own HTTP client

`Client(http_client=...)` builds on an httpx client of yours, for example to use a proxy. That client has to set the `User-Agent` itself and must not follow redirects.

An application with another HTTP stack skips the clients. The network builds each request and reads its answer, and the same pair exists for nearby stops, departures, arrivals and trips:

```python
request = network.locations_request(query)  # a LocationQuery
locations = network.parse_locations(response)
```

## Known limitations

The model classes are generated by [UniFFI](https://mozilla.github.io/uniffi-rs/), and type checkers stumble over the generated code in two places. Both work at runtime; a typed application needs an ignore comment there.

- `Place.ID("...")`, `Place.COORDINATE(...)` and the `TripTime` variants are rejected where a `Place` or `TripTime` is expected. Each takes its one value as a positional argument.
- pyright rejects `except Error:`. Catching a subclass such as `Error.NotFound` is accepted, and the fields of the subclasses are untyped.

`str()` of an error shows only its fields and is empty for errors without any, such as `Error.Blocked`. Use `repr()` in messages.

<!-- ANCHOR_END: site -->

## Development

The package is built with [maturin](https://www.maturin.rs) from the crate in `bindings/uniffi`. Inside the repository's nix shell:

```sh
uv sync                                          # build the Rust library, install into .venv
uv sync --reinstall-package publictransport      # again after a change to the Rust code
ruff check . && ruff format --check . && pyright
uv run pytest
```
