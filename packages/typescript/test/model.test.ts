// The types and most functions are generated from the Rust core. These
// tests check the shapes that its attributes ask for, and that each
// function crosses the boundary, not how the core derives a value.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  arrival,
  bundledNetwork,
  delaySeconds,
  departure,
  durationSeconds,
  expectedTime,
  type Location,
  localExpectedTime,
  localScheduledTime,
  localTime,
  locationsRequest,
  networkFromJson,
  type Place,
  parseDepartures,
  parseTrips,
  place,
  platform,
  RateLimitedError,
  Registry,
  sameStation,
  stationIds,
  stopId,
  type TripTime,
  tierAt,
  transfers,
  tripsRequest,
} from "@kiliankoe/publictransport";
import { fixture, foreignStop, ok, time } from "./support.ts";

const vvo = bundledNetwork("vvo-efa");

test("place is an ID, a coordinate or a location", () => {
  const position = { latitude: 51.04, longitude: 13.73 };
  const stop: Location = { ...foreignStop, network: "vvo-efa", coordinate: position };
  // The last one is a location, although it has what a coordinate has.
  const places: Place[] = ["de:14612:28", position, stop, { ...stop, ...position }];

  const origins = places.map((origin) => {
    const request = tripsRequest(vvo, { origin, destination: "de:14612:13" });
    return new URL(request.url).searchParams.get("name_origin");
  });

  const coordinate = "13.73:51.04:WGS84[dd.ddddd]";
  assert.deepEqual(origins, ["de:14612:28", coordinate, "8010085", "8010085"]);
  assert.equal(stopId(vvo, stop), "8010085");
  assert.deepEqual(place(vvo, { ...stop, kind: "address" }), position);
});

test("argument of the wrong shape is a type error that says what is wrong", () => {
  // A location may leave out its global IDs, but not its kind.
  const origin = { id: "8010085", name: "Dresden Hbf" } as Location;

  assert.throws(() => tripsRequest(vvo, { origin, destination: "de:14612:13" }), {
    name: "TypeError",
    message: "the query is not valid: missing field `kind`",
  });
});

test("key that is undefined is a missing key", () => {
  const network = { ...vvo, coverage: undefined, headers: undefined, timezone: undefined };
  const query = { origin: "a", destination: "b", time: undefined, page: undefined };

  const request = tripsRequest(network, query);

  assert.deepEqual(request, tripsRequest(vvo, { origin: "a", destination: "b" }));
  assert.deepEqual(
    locationsRequest(vvo, { text: "a", kinds: undefined }),
    locationsRequest(vvo, { text: "a" }),
  );
});

test("trip time is bound to the departure or to the arrival", () => {
  const times: [TripTime, string][] = [
    [{ departAt: time }, "dep"],
    [{ arriveBy: time }, "arr"],
  ];
  for (const [bound, expected] of times) {
    const request = tripsRequest(vvo, { origin: "a", destination: "b", time: bound });
    const sent = new URL(request.url).searchParams;

    assert.equal(sent.get("itdTripDateTimeDepArr"), expected);
    assert.equal(sent.get("itdTime"), "1424");
  }
});

test("times are dates and missing values are missing keys", () => {
  const departures = parseDepartures(vvo, ok(fixture("efa/vvo-departures.json")));
  const [first] = departures;
  assert.ok(first);

  assert.ok(first.scheduledTime instanceof Date);
  assert.equal(first.scheduledTime.toISOString(), "2026-11-16T13:24:00.000Z");
  assert.equal(first.line.mode, "bus");
  // A walk has no predicted times.
  const legs = parseTrips(vvo, ok(fixture("efa/vvo-trip.json"))).trips.flatMap((trip) => trip.legs);
  const walk = legs.find((leg) => leg.kind.type === "walk")?.departure;
  assert.ok(walk && !("realtimeTime" in walk));
  // A time before 1970 with a fraction is rounded down, as in the other languages.
  assert.equal(expectedTime({ ...walk, scheduledTime: new Date(-1500) }).getTime(), -2000);
});

test("local time has the offset that holds at the location", () => {
  const berlin = { ...foreignStop, timezone: "Europe/Berlin" };

  assert.equal(localTime(berlin, time), "2026-11-16T14:24:00+01:00");
  assert.equal(localTime(berlin, new Date("2026-07-16T13:24:00Z")), "2026-07-16T15:24:00+02:00");
  assert.equal(localTime(foreignStop, time), undefined);
  assert.equal(localTime({ ...foreignStop, timezone: "Mars/Olympus" }, time), undefined);
});

test("message of an error is the sentence of the core", () => {
  const tooMany = { status: 429, headers: [], body: new Uint8Array() };

  assert.throws(() => parseDepartures(vvo, tooMany), {
    constructor: RateLimitedError,
    message: "the server received too many requests",
  });
});

test("derived values come from the core", () => {
  const [first] = parseDepartures(vvo, ok(fixture("efa/vvo-departures.json")));
  const [trip] = parseTrips(vvo, ok(fixture("efa/vvo-trip.json"))).trips;
  const start = trip?.legs[0]?.departure;
  assert.ok(first && trip && start);

  assert.deepEqual(expectedTime(first), first.realtimeTime ?? first.scheduledTime);
  assert.deepEqual(expectedTime(start), start.realtimeTime ?? start.scheduledTime);
  assert.deepEqual(departure(trip), start);
  assert.deepEqual(arrival(trip), trip.legs.at(-1)?.arrival);
  assert.equal(transfers(trip), 1);
  assert.equal(localScheduledTime(first), "2026-11-16T14:24:00+01:00");
  assert.equal(localExpectedTime(first), "2026-11-16T14:25:00+01:00");
  assert.equal(delaySeconds(start), 60);
  assert.equal(platform(start), "14");
  assert.equal(durationSeconds(trip), 840);
  assert.throws(() => departure({ legs: [] }), TypeError);
  assert.deepEqual(stationIds(foreignStop), [{ scheme: "ifopt", value: "de:14612:28" }]);
  assert.ok(sameStation(foreignStop, start.stop));
});

test("registry finds networks by ID and by position", () => {
  const registry = Registry.bundled();
  const duesseldorf = { latitude: 51.2198, longitude: 6.7943 };

  assert.deepEqual(registry.get("vvo-efa"), vvo);
  assert.equal(registry.get("nowhere"), undefined);
  const covering = registry.covering(duesseldorf);
  assert.deepEqual(covering[0], { network: bundledNetwork("vrr-efa"), tier: "realtime" });
  assert.equal(tierAt(covering[0]?.network.coverage ?? {}, duesseldorf), "realtime");
  assert.deepEqual(new Registry([vvo]).networks(), [vvo]);
});

test("network is read from an entry of transport-apis", () => {
  const entry = {
    name: "Verkehrsverbund Rhein-Ruhr",
    type: { efa: true },
    options: { endpoint: "https://efa.vrr.de/standard" },
  };

  const network = networkFromJson("vrr", JSON.stringify(entry));

  assert.deepEqual(
    [network.id, network.protocol, network.endpoint],
    ["vrr", "efa", entry.options.endpoint],
  );
});
