// Lists the next departures at a stop on the HAFAS server of VBB, the
// transport association of Berlin and Brandenburg.

import Foundation
import PublicTransport

// Servers want to know who asks: name your application and a way to reach you.
let client = Client(userAgent: "publictransport-example (+https://github.com/kiliankoe/publictransport)")
let network = Network.bundled(.vbbHafas)

let stops = try await client.locations(in: network, matching: "S+U Alexanderplatz", kinds: [.stop])
guard let stop = stops.first else { fatalError("found no stop") }

/// The time of day at the stop. Times are `Date` values, and a location
/// has the time zone to show them in.
func localTime(at stop: Location, _ time: Date) -> String {
    time.formatted(Date.FormatStyle(time: .shortened, timeZone: stop.timeZone ?? .gmt))
}

print("Departures at \(stop.name):")
// From now on. `after:` asks for another time.
for departure in try await client.departures(in: network, at: stop, limit: 10) {
    // Only departures with real-time data have a delay.
    var delay = ""
    if let realtime = departure.realtimeTime {
        let minutes = Int(realtime.timeIntervalSince(departure.scheduledTime) / 60)
        delay = minutes < 0 ? "\(minutes)" : "+\(minutes)"
    }
    print(
        "  \(localTime(at: departure.stop, departure.scheduledTime)) \(delay)"
            + " \(departure.line.name) \(departure.direction ?? "?")"
            + (departure.cancelled ? " (cancelled)" : ""))
}
