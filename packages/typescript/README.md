<!-- ANCHOR: site -->
# publictransport for TypeScript

A TypeScript and JavaScript client for public transport APIs. It uses a shared model for stops, departures and trips that covers several server types, so an application can query any supported network the same way. It supports servers that speak EFA (used by many German transport associations), MOTIS (the routing engine behind [Transitous](https://transitous.org)), TRIAS 1.2, OJP 1.0, HAFAS mgate (the interface behind many operators' apps) and OpenTripPlanner 2 with its Transmodel API.

The package wraps the Rust library of the same name, compiled to WebAssembly. The Rust code builds the requests and parses the answers, and `fetch` sends them. It runs on Node.js from version 22, on Deno and on Bun. It has no dependencies and is an ES module only. The WebAssembly code is part of the JavaScript, so no runtime or bundler needs any setup to load it.

In a browser the package loads and works, but a page can reach few servers directly: most transport servers do not allow requests from other origins (CORS), and a browser may not let a script set the `User-Agent` that some servers require. A page therefore needs a proxy that it controls, see `fetch` below.

The package is on npm as `@kiliankoe/publictransport`:

```sh
pnpm add @kiliankoe/publictransport
```

```ts
import { bundledNetwork, Client } from "@kiliankoe/publictransport";

const client = new Client("my-app/1.0 (+https://example.org)");
const network = bundledNetwork("vvo-efa");

const [from, to] = await client.locations(network, "Hauptbahnhof", { kinds: ["stop"] });
if (!(from && to)) throw new Error("no stops found");

for (const departure of await client.departures(network, from, { limit: 10 })) {
  // The time on the clocks at the stop. Without a time zone it would be the one of this machine.
  const time = departure.scheduledTime.toLocaleTimeString("en-GB", {
    timeZone: departure.stop.timezone,
    timeStyle: "short",
  });
  console.log(`${time} ${departure.line.name} ${departure.direction ?? ""}`);
}

const found = await client.trips(network, from, to, { limit: 5 });
// The same arguments again, with the reference to the trips after these.
const more = await client.trips(network, from, to, { limit: 5, page: found.later });
```

The [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/typescript) have a short program for each kind of request.

`bundledNetwork("vvo-efa")` gives one of the networks that ship with the package; the type `BundledNetwork` lists their IDs. `Registry.bundled()` holds them all, for a list to choose from: `get("vvo-efa")` finds one by an ID that is only known at run time, and `covering({ latitude, longitude })` lists the networks for a position, each with the tier of its coverage there. A `Network` can also be written by hand, as `{ id: ..., name: ..., endpoint: ..., protocol: "efa", timezone: "Europe/Berlin" }`, and `networkFromJson(id, json)` reads further ones from entries in the format of [transport-apis](https://github.com/public-transport/transport-apis).

The model types are plain objects, described by interfaces, so a spread makes a changed one: `{ ...network, headers: [...(network.headers ?? []), { name: "X-Api-Key", value: key }] }`. A value that the server did not send is a missing key, not `null`, and a key that is `undefined` counts as missing where a value is passed in. Enumerations are strings, such as `"stop"` or `"highSpeedTrain"`, and the kind of a leg is told apart by `leg.kind.type`. What the other languages have as methods are functions here: `localTime(location, time)`, `expectedTime(departure)`, `localScheduledTime(departure)`, `localExpectedTime(departure)`, `delaySeconds(departure)`, `platform(departure)`, `departure(trip)`, `arrival(trip)`, `transfers(trip)`, `durationSeconds(trip)`, `stationIds(location)`, `sameStation(location, other)`, `stopId(network, location)`, `place(network, location)` and `tierAt(coverage, position)`.

A place is a `Location` from a search, a departure or a trip, the ID of a stop on the network that is asked as a string, or a coordinate as `{ latitude, longitude }`. `departures` takes a location or an ID, `trips` any two places. A location that another network returned works as well: for a trip always if it has a coordinate, for departures where the asked network can name the stop, which EFA, TRIAS and OJP can for stops with a global ID. The options after the places are the other fields of the query, and for a trip `departAt` or `arriveBy` as its time: `client.trips(network, from, to, { arriveBy: date, limit: 3 })`. Each call also takes the whole query as one object (`LocationQuery`, `DeparturesQuery`, `TripsQuery`), as in `client.departures(network, { stop, limit: 10 })`. A limit is a guide for the server and not a maximum.

Times are `Date` values. A location has the name of its time zone as `timezone`, where the server tells it, so `Intl` and `toLocaleTimeString` show a time as the clocks there do, as above. `localTime(location, time)` gives that time as text in ISO 8601 format with the offset that holds there at that time, such as `2026-11-19T14:08:00+01:00`, or `undefined` where no time zone is known for it. `localScheduledTime` and `localExpectedTime` give the times of a departure or of an end of a leg in the same way, at its stop. `delaySeconds` is how late the vehicle is, in seconds, and `undefined` without real-time data. `platform` is the real-time platform if there is one, otherwise the scheduled one. `durationSeconds(trip)` is how long a trip takes, in seconds. `localTime` gives text because JavaScript has no type for a time with an offset; `Temporal.ZonedDateTime` can be made from it where that exists. An EFA network needs its `timezone` to send a query time. A TRIAS or OJP network needs a `requestorRef`, which public servers document, e.g. `OpenService`. A HAFAS network needs `hafas` and its `timezone`; the bundled ones have both.

`trips` returns a `Trips` value: the list is in `trips`, and `earlier` and `later` are references for more. Pass one as `page` with the same other arguments to continue the search. An answer from an OpenTripPlanner server can be empty at night and still have a `later` reference to follow.

Failures that the server or the query cause are thrown as subclasses of `PublicTransportError`, such as `NotFoundError` or `RateLimitedError`; the `message` is a sentence in English, and `detail` has what the server said where there is more to tell. Failures of the HTTP exchange are thrown as `fetch` throws them, usually as a `TypeError`. An answer above 16 MB is a `ResponseTooLargeError`. The error of a status that is no success says what it means, whatever the body holds: a redirect is a `MovedError`, 401 an `UnauthorizedError`, 403 a `BlockedError`, 429 a `RateLimitedError`, a status from 500 an `UnavailableError`, and any other a `ServerError` with the `status`. A `NotFoundError` is a stop or a place that the server does not know, not the status 404. A network or a query of the wrong shape, such as a negative limit, is a `TypeError` before a request is sent. The client does not retry, limit its request rate or cache, and it sets no time limit. Every call takes a `signal` among its options, which cancels the request: `client.departures(network, stop, { signal: AbortSignal.timeout(30_000) })`. A cancelled or timed-out request rejects with the `DOMException` of the signal, an `AbortError` or a `TimeoutError`.

One client serves every network, so create one and keep it. It can be built on a `fetch` of your own, `new Client(userAgent, { fetch })`, for example to send the requests through a proxy. A page in a browser needs that for most servers: the function gets the URL of the transport server and the request to send, passes both to a proxy on the page's own origin, and returns its answer. It has to pass on `redirect: "manual"` and the `signal`, and the proxy must not follow redirects either.

Some networks need more than their entry holds. Entur asks every client for its name in the header `ET-Client-Name` (`company-application`) and limits clients without it: add it to the headers of the bundled network with a spread, as above.

An application with its own HTTP stack can skip the client: `locationsRequest(network, query)` returns the request to send, and `parseLocations(network, response)` reads the answer. Departures and trips have the same pair. The headers of both are lists of `{ name, value }`, since a name can repeat: `new Headers(request.headers.map(({ name, value }) => [name, value]))` makes those of a `fetch`.

## Known limitations

The model types and the functions on them are generated from the Rust code by [wasm-bindgen](https://wasm-bindgen.github.io/wasm-bindgen/) and [tsify](https://github.com/madonoharu/tsify), which shows in a few places.

- The documentation of the model types comes from the Rust code, so it links and names things as Rust does: ``[`Location::local_time`]`` is `localTime`, `scheduled_time` is `scheduledTime`, `None` is a missing key, and `Error` is `PublicTransportError`.
- A network crosses into the WebAssembly module with every call. For one with large coverage areas, such as Transitous, that takes one to two milliseconds.
- The fields that have a default, such as the `headers` of a `Network` or the `globalIds` of a `Location`, are optional in their types, so that a value written by hand can leave them out. A value from the package always has them.
- A `Registry` lives in the WebAssembly module. Its memory is freed when the object is collected, or at once with `free()`. Its type names `Symbol.dispose`, so a project that checks the types of its dependencies (`skipLibCheck: false`) needs `ESNext.Disposable` in `lib`, or the types of Node.js.
- A page with a Content Security Policy has to allow WebAssembly with `'wasm-unsafe-eval'`.
- The types `Thrown` and `Failure` describe what the WebAssembly module throws. The package turns that into a `PublicTransportError`, so they are no part of the API.

<!-- ANCHOR_END: site -->

## Development

The package is built with pnpm, in the repository's nix shell (`nix develop`). `build.sh` runs `cargo` to build the crate in `bindings/wasm` for WebAssembly, lets wasm-bindgen write the JavaScript and the TypeScript declarations of the model into `src/generated`, shrinks the module with `wasm-opt`, widens every optional key in the declarations to `| undefined`, and writes the module that holds the WebAssembly code as base64 text. Then `tsc` compiles the hand-written code in `src` into `dist`: the client on `fetch` and the error classes.

```sh
pnpm install
pnpm lint         # Biome, also for the examples
pnpm build        # build the WebAssembly module, generate the model, compile
pnpm typecheck    # the tests and the examples against the built package
pnpm test         # the tests, against the built package
```

`just typescript` runs all of these from the root of the repository. The tests run in Node's test runner and import the built package by its name, so build before testing. They also pass in Deno (`deno test -A --no-check test/*.test.ts`) and Bun (`bun test test/`). The examples in `examples/typescript` use the package of this checkout through a link.

The version of the `wasm-bindgen` crate is pinned in `crates/core/Cargo.toml` and `bindings/wasm/Cargo.toml` to the version of the `wasm-bindgen` command that the nix shell has, because the two have to match. `build.sh` compares them first and says what to change when a flake update brings a new command. Dependabot leaves the crate and those released with it alone.
