// Searches for places by name and near a position on the EFA server of
// VVO, the transport association of Dresden.

import { bundledNetwork, Client, distanceMeters } from "@kiliankoe/publictransport";

// Servers want to know who asks: name your application and a way to reach you.
const client = new Client(
  "publictransport-example (+https://github.com/kiliankoe/publictransport)",
);
const network = bundledNetwork("vvo-efa");

// Every kind of place. `{ kinds: ["stop"] }` as a third argument asks for stops only.
const locations = await client.locations(network, "Dresden Zwinger");

// The best match comes first.
for (const location of locations.slice(0, 5)) {
  const { coordinate } = location;
  const position = coordinate
    ? ` at ${coordinate.latitude.toFixed(4)}, ${coordinate.longitude.toFixed(4)}`
    : "";
  console.log(`${location.kind}: ${location.name} (${location.id})${position}`);
}

// The stops within 300 m of the best match, the nearest first. The distance
// is the straight line.
const center = locations[0]?.coordinate;
for (const stop of center ? await client.nearby(network, center, { radiusMeters: 300 }) : []) {
  if (center && stop.coordinate) {
    console.log(`${distanceMeters(center, stop.coordinate).toFixed(0)} m: ${stop.name}`);
  }
}
