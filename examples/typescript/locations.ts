// Searches for places by name on the EFA server of VVO, the transport
// association of Dresden.

import { bundledNetwork, Client } from "@kiliankoe/publictransport";

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
