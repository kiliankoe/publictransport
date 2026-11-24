// Lists the bundled networks that cover a position. This sends no request:
// the networks and their areas ship with the package.

import PublicTransport

let registry = Registry.bundled()
print("\(registry.networks().count) networks are in the registry.")

let duesseldorf = Coordinate(latitude: 51.2198, longitude: 6.7943)
// The best coverage comes first: real-time data, then complete timetables,
// then any data.
print("At Düsseldorf Hauptbahnhof:")
for covering in registry.covering(at: duesseldorf) {
    let network = covering.network
    print("  \(covering.tier): \(network.name) (\(network.id), \(network.protocol))")
}

// A network by its ID, as an application would store it.
if let network = registry.network(id: "vrr-efa") {
    print("vrr-efa is \(network.name) at \(network.endpoint)")
}
