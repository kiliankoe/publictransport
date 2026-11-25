// Searches for places by name and near a position on the EFA server of
// VVO, the transport association of Dresden.

import Foundation
import PublicTransport

// Servers want to know who asks: name your application and a way to reach you.
let client = Client(userAgent: "publictransport-example (+https://github.com/kiliankoe/publictransport)")
let network = TransitNetwork.bundled(.vvoEfa)

// Every kind of place. `kinds: [.stop]` asks for stops only.
let locations = try await client.locations(in: network, matching: "Dresden Zwinger")

// The best match comes first.
for location in locations.prefix(5) {
    var position = ""
    if let coordinate = location.coordinate {
        position = String(format: " at %.4f, %.4f", coordinate.latitude, coordinate.longitude)
    }
    print("\(location.kind): \(location.name) (\(location.id))\(position)")
}

// The stops within 300 m of the best match, the nearest first. The distance
// is the straight line.
if let center = locations.first?.coordinate {
    for stop in try await client.nearby(in: network, at: center, radiusMeters: 300) {
        guard let coordinate = stop.coordinate else { continue }
        print(String(format: "%.0f m: %@", center.distanceMeters(to: coordinate), stop.name))
    }
}
