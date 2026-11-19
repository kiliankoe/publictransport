// Searches for places by name on the EFA server of VVO, the transport
// association of Dresden.

import Foundation
import PublicTransport

// Servers want to know who asks: name your application and a way to reach you.
let client = Client(userAgent: "publictransport-example (+https://github.com/kiliankoe/publictransport)")
let network = Network.bundled(.vvoEfa)

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
