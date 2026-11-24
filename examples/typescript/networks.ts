// Lists the bundled networks that cover a position. This sends no request:
// the networks and their areas ship with the package.

import { Registry } from "@kiliankoe/publictransport";

const registry = Registry.bundled();
console.log(`${registry.networks().length} networks are in the registry.`);

const duesseldorf = { latitude: 51.2198, longitude: 6.7943 };
// The best coverage comes first: real-time data, then complete timetables,
// then any data.
console.log("At Düsseldorf Hauptbahnhof:");
for (const { network, tier } of registry.covering(duesseldorf)) {
  console.log(`  ${tier}: ${network.name} (${network.id}, ${network.protocol})`);
}

// A network by its ID, as an application would store it.
const network = registry.get("vrr-efa");
if (network) {
  console.log(`vrr-efa is ${network.name} at ${network.endpoint}`);
}
