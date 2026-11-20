// Plans trips between two stops on the OTP server of Entur, the journey
// planner for Norway, and asks for the trips after the first ones.

import {
  arrival,
  bundledNetwork,
  Client,
  departure,
  expectedTime,
  type LegEnd,
  type Trip,
  transfers,
} from "@kiliankoe/publictransport";

// The expected time of day at the stop: the real-time one if there is one.
// Times are `Date` values. With the time zone of the stop, where the server
// names it, one reads as on the clocks there.
function clock(end: LegEnd): string {
  const { timezone } = end.stop;
  return timezone
    ? expectedTime(end).toLocaleTimeString("en-GB", { timeZone: timezone, timeStyle: "short" })
    : "??:??";
}

function printTrip(trip: Trip) {
  const start = departure(trip);
  const end = arrival(trip);
  if (!(start && end)) {
    return;
  }
  console.log(`  ${clock(start)} to ${clock(end)}, ${transfers(trip)} change(s)`);
  for (const leg of trip.legs) {
    const how =
      leg.kind.type === "transit"
        ? `${leg.kind.line.name} to ${leg.kind.direction ?? "?"}`
        : leg.kind.type;
    console.log(
      `    ${clock(leg.departure)} ${leg.departure.stop.name}` +
        ` -> ${clock(leg.arrival)} ${leg.arrival.stop.name}: ${how}`,
    );
  }
}

// Servers want to know who asks: name your application and a way to reach you.
const client = new Client(
  "publictransport-example (+https://github.com/kiliankoe/publictransport)",
);
const entur = bundledNetwork("entur");
// Entur asks every client for its name, as `company-application`.
const network = {
  ...entur,
  headers: [...(entur.headers ?? []), { name: "ET-Client-Name", value: "publictransport-example" }],
};

const [origin] = await client.locations(network, "Oslo S", { kinds: ["stop"] });
const [destination] = await client.locations(network, "Lillehammer stasjon", { kinds: ["stop"] });
if (!(origin && destination)) {
  throw new Error("a stop was not found");
}

// Leaving now. `departAt` or `arriveBy` next to the limit asks for another
// time. An end can also be a coordinate,
// `{ latitude: ..., longitude: ... }`, or the ID of a stop as a string.
const found = await client.trips(network, origin, destination, { limit: 3 });
console.log(`Trips from ${origin.name} to ${destination.name}:`);
found.trips.forEach(printTrip);

// The same arguments again, with the reference to the trips after these.
if (found.later) {
  console.log("Later trips:");
  const more = await client.trips(network, origin, destination, { limit: 3, page: found.later });
  more.trips.forEach(printTrip);
}
