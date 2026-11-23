// Building requests and parsing answers is the Rust core's job and tested
// there, so the tests compare with what the core itself returns.

import assert from "node:assert/strict";
import type { ServerResponse } from "node:http";
import { test } from "node:test";
import {
  arrivalsRequest,
  BlockedError,
  Client,
  type DeparturesQuery,
  departuresRequest,
  type HttpRequest,
  type HttpResponse,
  InvalidNetworkError,
  InvalidQueryError,
  type LocationQuery,
  locationsRequest,
  MovedError,
  type NearbyQuery,
  type Network,
  NotFoundError,
  nearbyRequest,
  ParseError,
  PublicTransportError,
  parseArrivals,
  parseDepartures,
  parseLocations,
  parseNearby,
  parseTrips,
  RateLimitedError,
  ResponseTooLargeError,
  ServerError,
  type TripsQuery,
  tripsRequest,
  UnauthorizedError,
  UnavailableError,
  UnsupportedError,
} from "@kiliankoe/publictransport";
import { efa, fixture, foreignStop, ok, serving, time } from "./support.ts";

const USER_AGENT = "publictransport-tests/1.0";

const otherForeignStop = {
  ...foreignStop,
  id: "8010089",
  globalIds: [{ scheme: "ifopt" as const, value: "de:14612:13" }],
  name: "Dresden Albertplatz",
};
const hbf = "de:14612:28";
const albertplatz = "de:14612:13";
const coordinate = { latitude: 51.04, longitude: 13.73 };

// TRIAS is the protocol whose requests have a body and a header of their
// own, next to the one of the network. It also sends the limit of a search.
function trias(url: string): Network {
  return {
    id: "trias",
    name: "TRIAS",
    endpoint: `${url}/trias`,
    protocol: "trias",
    requestorRef: "OpenService",
    headers: [{ name: "X-Api-Key", value: "secret" }],
  };
}

/** The reference to the trips after those of the trips fixture. */
const later = parseTrips(efa("http://localhost"), ok(fixture("efa/vvo-trip.json"))).later;
assert.ok(later);

/** One call of the client, next to the request the core builds for it. */
interface Case {
  name: string;
  fixture: string;
  network?: (url: string) => Network;
  request: (network: Network) => HttpRequest;
  parse: (network: Network, response: HttpResponse) => unknown;
  call: (client: Client, network: Network) => Promise<unknown>;
}

function locations(
  name: string,
  query: LocationQuery,
  call: Case["call"],
  rest: Partial<Case> = {},
): Case {
  const request = (network: Network) => locationsRequest(network, query);
  return {
    name,
    fixture: "efa/vvo-stopfinder.json",
    request,
    parse: parseLocations,
    call,
    ...rest,
  };
}

function nearby(name: string, query: NearbyQuery, call: Case["call"]): Case {
  const request = (network: Network) => nearbyRequest(network, query);
  return { name, fixture: "efa/vvo-nearby.json", request, parse: parseNearby, call };
}

function departures(name: string, query: DeparturesQuery, call: Case["call"]): Case {
  const request = (network: Network) => departuresRequest(network, query);
  return { name, fixture: "efa/vvo-departures.json", request, parse: parseDepartures, call };
}

function arrivals(name: string, query: DeparturesQuery, call: Case["call"]): Case {
  const request = (network: Network) => arrivalsRequest(network, query);
  return { name, fixture: "efa/vvo-arrivals.json", request, parse: parseArrivals, call };
}

function trips(name: string, query: TripsQuery, call: Case["call"]): Case {
  const request = (network: Network) => tripsRequest(network, query);
  return { name, fixture: "efa/vvo-trip.json", request, parse: parseTrips, call };
}

const calls: Case[] = [
  locations("locations by query", { text: "Hauptbahnhof" }, (client, network) =>
    client.locations(network, { text: "Hauptbahnhof" }),
  ),
  locations(
    "locations by text, kinds and limit, with a body and headers",
    { text: "Hauptbahnhof", limit: 3, kinds: ["stop"] },
    (client, network) =>
      // A key that is `undefined` is a missing one, also for the type check.
      client.locations(network, "Hauptbahnhof", { limit: 3, kinds: ["stop"], signal: undefined }),
    { fixture: "trias/vvo-locations.xml", network: trias },
  ),
  nearby(
    "stops near a position, with radius and limit",
    { position: coordinate, radiusMeters: 500, limit: 5 },
    (client, network) => client.nearby(network, coordinate, { radiusMeters: 500, limit: 5 }),
  ),
  departures(
    "departures by query, at a location of another network",
    { stop: foreignStop },
    (client, network) => client.departures(network, { stop: foreignStop }),
  ),
  departures("departures at an ID, with time and limit", { stop: hbf, time, limit: 7 }, (c, n) =>
    c.departures(n, hbf, { time, limit: 7 }),
  ),
  arrivals("arrivals at an ID, with time and limit", { stop: hbf, time, limit: 7 }, (c, n) =>
    c.arrivals(n, hbf, { time, limit: 7 }),
  ),
  trips("trips by query", { origin: hbf, destination: albertplatz }, (client, network) =>
    client.trips(network, { origin: hbf, destination: albertplatz }),
  ),
  trips(
    "trips between locations, departing at a time, with limit",
    {
      origin: foreignStop,
      destination: otherForeignStop,
      time: { departAt: time },
      limit: 3,
    },
    (client, network) =>
      client.trips(network, foreignStop, otherForeignStop, { departAt: time, limit: 3 }),
  ),
  trips(
    "trips from an ID to a coordinate, arriving by a time",
    { origin: hbf, destination: coordinate, time: { arriveBy: time } },
    (client, network) => client.trips(network, hbf, coordinate, { arriveBy: time }),
  ),
  trips(
    "trips of the next page",
    { origin: hbf, destination: albertplatz, page: later },
    (client, network) => client.trips(network, hbf, albertplatz, { page: later }),
  ),
];

for (const { name, network: networkAt = efa, request, parse, call, ...rest } of calls) {
  test(`sends the request of the core and returns what the core parses: ${name}`, async () => {
    const body = fixture(rest.fixture);
    await serving([{ body }], async (url, received) => {
      const network = networkAt(url);
      const expected = request(network);

      assert.deepEqual(await call(new Client(USER_AGENT), network), parse(network, ok(body)));

      assert.equal(received.length, 1);
      const [sent] = received;
      assert.equal(sent?.method, expected.method);
      assert.equal(new URL(sent?.url ?? "", url).href, expected.url);
      assert.deepEqual(sent?.body, expected.body ?? new Uint8Array());
      for (const header of expected.headers) {
        assert.equal(sent?.headers[header.name.toLowerCase()], header.value, header.name);
      }
      assert.equal(sent?.headers["user-agent"], USER_AGENT);
    });
  });
}

test("user agent of the network replaces the one of the client", async () => {
  await serving([{ body: fixture("efa/vvo-stopfinder.json") }], async (url, received) => {
    const network = { ...efa(url), headers: [{ name: "User-Agent", value: "network/1.0" }] };

    await new Client(USER_AGENT).locations(network, "Hauptbahnhof");

    assert.equal(received[0]?.headers["user-agent"], "network/1.0");
  });
});

const moved = [{ name: "Location", value: "/new" }];
const retryAfter = [{ name: "Retry-After", value: "120" }];

for (const [name, status] of [
  ["redirect", 302],
  ["unavailable", 503],
] as const) {
  test(`error of the core is thrown and the request is sent once: ${name}`, async () => {
    // The second answer is what a followed redirect or a retry would get.
    const answers = [
      { status, headers: { location: "/new" } },
      { body: fixture("efa/vvo-departures.json") },
    ];
    await serving(answers, async (url, received) => {
      const network = efa(url);
      // Status and headers have to reach the core for the same error.
      let expected: unknown;
      try {
        parseDepartures(network, { status, headers: moved, body: new Uint8Array() });
      } catch (error) {
        expected = error;
      }
      assert.ok(expected instanceof PublicTransportError);

      const call = new Client(USER_AGENT).departures(network, hbf);
      const thrown = await call.catch((error) => error);

      assert.deepEqual(thrown, expected);
      assert.equal(received.length, 1);
    });
  });
}

test("each error of the core is thrown as its class, with its details", () => {
  const network = efa("http://localhost");
  const body = new TextEncoder().encode("teapot");
  const answer = (status: number) => () =>
    parseDepartures(network, { status, headers: moved, body });
  const unknownStop = ok(fixture("efa/vvo-departures-unknown-stop.json"));

  assert.throws(answer(302), (thrown) => {
    assert.ok(thrown instanceof MovedError);
    assert.equal(thrown.location, "/new");
    assert.equal(thrown.message, "the endpoint has moved to /new");
    assert.equal(thrown.name, "MovedError");
    return true;
  });
  assert.throws(answer(418), (thrown) => {
    assert.ok(thrown instanceof ServerError);
    assert.deepEqual([thrown.status, thrown.detail], [418, "teapot"]);
    return true;
  });
  assert.throws(answer(401), UnauthorizedError);
  assert.throws(answer(403), BlockedError);
  assert.throws(
    () => parseDepartures(network, { status: 429, headers: retryAfter, body }),
    (thrown) => thrown instanceof RateLimitedError && thrown.retryAfterSeconds === 120,
  );
  assert.throws(answer(503), UnavailableError);
  assert.throws(() => parseDepartures(network, unknownStop), NotFoundError);
  assert.throws(() => parseDepartures(network, ok(new Uint8Array())), ParseError);
  assert.throws(() => departuresRequest(network, { stop: coordinate }), InvalidQueryError);
  assert.throws(
    () => locationsRequest({ ...network, protocol: "trias" }, { text: "Hauptbahnhof" }),
    InvalidNetworkError,
  );
  assert.throws(
    () => locationsRequest({ ...network, unsupported: ["locations"] }, { text: "Hauptbahnhof" }),
    UnsupportedError,
  );
});

test("failed exchange is the error of fetch", async () => {
  // Nothing listens on the port of a closed server.
  let closed = "";
  await serving([], (url) => {
    closed = url;
    return Promise.resolve();
  });

  const call = new Client(USER_AGENT).locations(efa(closed), "Hauptbahnhof");
  const thrown = await call.catch((error) => error);

  assert.ok(thrown instanceof TypeError, `${thrown}`);
  assert.ok(!(thrown instanceof PublicTransportError));
});

test("redirect that a browser hides is a moved endpoint", async () => {
  // A browser answers a redirect that is not followed with this, and
  // a client can be built on a `fetch` of the caller.
  const hidden = { type: "opaqueredirect", status: 0, headers: new Headers(), body: null };
  // A `fetch` refuses a call with a `this`, as the one of a browser does.
  function fetchOfCaller(this: unknown) {
    assert.equal(this, undefined);
    return Promise.resolve(hidden as Response);
  }
  const client = new Client(USER_AGENT, { fetch: fetchOfCaller });

  await assert.rejects(client.locations(efa("http://localhost"), "Hauptbahnhof"), MovedError);
});

test("answer above 16 MB is refused", async () => {
  // Without a `Content-Length`, as a hostile server would send it.
  const tooLarge = (response: ServerResponse) => {
    const megabyte = new Uint8Array(1024 * 1024).fill(32);
    for (let sent = 0; sent < 17; sent++) {
      response.write(megabyte);
    }
    response.end();
  };
  await serving([tooLarge], async (url) => {
    const call = new Client(USER_AGENT).locations(efa(url), "Hauptbahnhof");

    await assert.rejects(call, ResponseTooLargeError);
  });
});

test("aborting the signal cancels the request", async () => {
  const controller = new AbortController();
  // The promise is rejected when the signal aborts either way, so the test
  // asks the server whether the connection went away before its answer.
  let cancelled: Promise<boolean> | undefined;
  const late = (response: ServerResponse) => {
    const answer = setTimeout(() => response.end(fixture("efa/vvo-stopfinder.json")), 500);
    cancelled = new Promise((resolve) => {
      response.on("close", () => {
        clearTimeout(answer);
        resolve(!response.writableEnded);
      });
    });
    controller.abort();
  };
  await serving([late], async (url) => {
    const call = new Client(USER_AGENT).locations(efa(url), "Hauptbahnhof", {
      signal: controller.signal,
    });

    await assert.rejects(call, { name: "AbortError" });
    assert.equal(await cancelled, true);
  });
});

test("invalid call is refused before a request", async () => {
  await serving([{}], async (url, received) => {
    const client = new Client(USER_AGENT);
    const network = trias(url);

    await assert.rejects(client.locations(network, "Hauptbahnhof", { limit: -1 }), TypeError);
    await assert.rejects(client.departures(network, hbf, { limit: 1.5 }), TypeError);
    // What JavaScript lets through where TypeScript would not.
    const noTime = "now" as never;
    await assert.rejects(client.trips(network, hbf, albertplatz, { departAt: noTime }), TypeError);

    assert.equal(received.length, 0);
  });
});

test("user agent that no request can carry is refused", () => {
  for (const userAgent of ["", " \t", "my\napp", "my-app ✓"]) {
    assert.throws(() => new Client(userAgent), TypeError, userAgent);
  }
});
