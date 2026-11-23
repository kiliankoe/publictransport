<!-- ANCHOR: site -->
# publictransport for TypeScript

A TypeScript and JavaScript client for public transport APIs. One model for stops, departures and trips covers servers that speak EFA, MOTIS, TRIAS 1.2, OJP 1.0, HAFAS mgate or OpenTripPlanner 2, so an application queries every supported network the same way.

```sh
pnpm add @kiliankoe/publictransport
```

It runs on Node.js from version 22, on Deno and on Bun, has no dependencies and is an ES module only. The package wraps the Rust library of the same name as WebAssembly, which builds the requests and parses the answers, and `fetch` sends them. The WebAssembly code is part of the JavaScript, so nothing needs setup to load it.

```ts
import { bundledNetwork, Client } from "@kiliankoe/publictransport";

const client = new Client("my-app/1.0 (+https://example.org)");
const network = bundledNetwork("vvo-efa");

const [from, to] = await client.locations(network, "Hauptbahnhof", { kinds: ["stop"] });
if (!(from && to)) throw new Error("no stops found");

for (const departure of await client.departures(network, from, { limit: 10 })) {
  console.log(departure.scheduledTime, departure.line.name, departure.direction ?? "");
}
```

`arrivals` takes the same arguments and lists what arrives at the stop. An arrival has an `origin`, where the vehicle comes from, in place of the `direction`.

```ts
const arrivals = await client.arrivals(network, from, { limit: 10 });
```

One client serves every network, so create one and keep it. The [examples](https://github.com/kiliankoe/publictransport/tree/main/examples/typescript) have a program for each kind of request.

## Trips

```ts
import { durationSeconds, transfers } from "@kiliankoe/publictransport";

const found = await client.trips(network, from, to, { limit: 5 });
for (const trip of found.trips) {
  console.log(durationSeconds(trip), "seconds,", transfers(trip), "changes");
}

// The same arguments again, with the reference to the trips after these.
const more = await client.trips(network, from, to, { limit: 5, page: found.later });
```

An end of a trip is a `Location`, the ID of a stop as a string or a coordinate. `departAt` and `arriveBy` ask for another time than now.

```ts
const home = { latitude: 51.04, longitude: 13.73 };
await client.trips(network, home, "de:14612:28", { arriveBy: tomorrowAtNine });
```

Each call also takes the whole query as one object, as in `client.departures(network, { stop, limit: 10 })`.

## Networks

```ts
import { Registry } from "@kiliankoe/publictransport";

const registry = Registry.bundled();
const network = registry.get("vrr-efa"); // undefined if no network has this ID

// The networks for a position, the best coverage first.
for (const { network, tier } of registry.covering({ latitude: 51.22, longitude: 6.79 })) {
  console.log(tier, network.name);
}
```

The [list of bundled networks](https://kiliankoe.github.io/publictransport/networks.html) also says what a network of your own needs. Write one as `{ id: ..., name: ..., endpoint: ..., protocol: "efa", timezone: "Europe/Berlin" }`, or read one from an entry of [transport-apis](https://github.com/public-transport/transport-apis) with `networkFromJson(id, json)`. The model types are plain objects, so a spread adds a header for a key or a client name:

```ts
const entur = bundledNetwork("entur");
const named = {
  ...entur,
  headers: [...(entur.headers ?? []), { name: "ET-Client-Name", value: "mycompany-myapp" }],
};
```

## The model

The model types are interfaces for plain objects. A value that the server did not send is a missing key, not `null`. Enumerations are strings, such as `"stop"` or `"highSpeedTrain"`, and the kind of a leg is told apart by `leg.kind.type`. What the other languages have as methods are functions here:

```ts
import {
  delaySeconds,
  expectedTime,
  localScheduledTime,
  platform,
} from "@kiliankoe/publictransport";

expectedTime(departure); // the real-time one, if there is one
delaySeconds(departure); // undefined without real-time data
platform(departure); // the real-time platform, otherwise the scheduled one
localScheduledTime(departure); // "2026-11-19T14:08:00+01:00", on the clocks at the stop
```

The others are `localExpectedTime`, `localTime(location, time)`, `departure(trip)`, `arrival(trip)`, `stationIds(location)`, `sameStation(location, other)`, `stopId(network, location)`, `place(network, location)` and `tierAt(coverage, position)`.

Times are `Date` values. The local times are text in ISO 8601 format, because JavaScript has no type for a time with an offset. To print a time, `Intl` does it from the `Date` and the time zone of the stop:

```ts
departure.scheduledTime.toLocaleTimeString("en-GB", {
  timeZone: departure.stop.timezone,
  timeStyle: "short",
});
```

## Disruptions and hints

A departure and a leg of a trip have `remarks`: the texts that the server sends with them. Every protocol but MOTIS fills them. A remark is one of two kinds. A disruption is a message from the incident system of the operator, such as a closed road or a broken elevator. A hint is a fixed property of the ride, such as air conditioning or a note for wheelchair users.

```ts
for (const remark of departure.remarks ?? []) {
  if (remark.kind === "disruption") {
    console.log(remark.title ?? remark.text);
    console.log(remark.url); // a page with more about it, or undefined
    console.log(remark.validFrom, remark.validUntil); // undefined where the server does not say
  }
}
```

At least one of `title` and `text` is set. `text` is plain text without HTML, in the language of the server. The server sends a disruption with every departure and leg it concerns, so an application that shows each message once tells them apart by `id`. A hint has no `id`. It has the `code` that the server gives its kind, such as `A__KL` for air conditioning on German EFA servers, for an application that shows icons for the ones it knows. Operators define these codes, so they differ between networks.

## Errors

```ts
import { NotFoundError, PublicTransportError } from "@kiliankoe/publictransport";

try {
  await client.departures(network, "no-such-stop", { signal: AbortSignal.timeout(30_000) });
} catch (error) {
  if (error instanceof NotFoundError) {
    // the server does not know the stop
  } else if (error instanceof PublicTransportError) {
    console.log(error.message, error.detail); // a sentence in English, and what the server said
  }
}
```

What the server or the query cause is a subclass of `PublicTransportError`: besides `NotFoundError`, a redirect is a `MovedError`, the status 401 an `UnauthorizedError`, 403 a `BlockedError`, 429 a `RateLimitedError` with `retryAfterSeconds`, how long the server asks to wait, a status from 500 an `UnavailableError`, and any other a `ServerError`. A failed HTTP exchange is thrown as `fetch` throws it, and an answer above 16 MB is a `ResponseTooLargeError`. A network or a query of the wrong shape is a `TypeError` before a request is sent.

The client sets no time limit. Every call takes a `signal`, which cancels the request and rejects with the `DOMException` of the signal.

## Browsers and your own `fetch`

In a browser the package works, but a page can reach few servers directly: most transport servers do not allow requests from other origins (CORS), and a browser may not let a script set the `User-Agent`. A page therefore needs a proxy on its own origin, which a `fetch` of your own sends the requests through:

```ts
const client = new Client("my-app/1.0", {
  fetch: (url, init) => fetch(`/proxy?url=${encodeURIComponent(String(url))}`, init),
});
```

The function has to pass on `redirect: "manual"` and the `signal`, as `init` above does, and the proxy must not follow redirects either.

An application with another HTTP stack skips the client. A pair of functions builds each request and reads its answer, and the same pair exists for departures, arrivals and trips:

```ts
const request = locationsRequest(network, query); // a LocationQuery
const locations = parseLocations(network, response);
```

The headers of both are lists of `{ name, value }`, since a name can repeat.

## Known limitations

The model types and the functions on them are generated from the Rust code by [wasm-bindgen](https://wasm-bindgen.github.io/wasm-bindgen/) and [tsify](https://github.com/madonoharu/tsify), which shows in a few places.

- The documentation of the model types comes from the Rust code, so it links and names things as Rust does: ``[`Location::local_time`]`` is `localTime`, `scheduled_time` is `scheduledTime`, `None` is a missing key, and `Error` is `PublicTransportError`.
- A network crosses into the WebAssembly module with every call. For one with large coverage areas, such as Transitous, that takes one to two milliseconds.
- The fields that have a default, such as the `headers` of a `Network`, the `globalIds` of a `Location` or the `remarks` of a `Departure`, are optional in their types, so that a value written by hand can leave them out. A value from the package always has them.
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
