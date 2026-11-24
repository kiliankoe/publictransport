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
let departures = try await client.departures(in: network, at: stop, limit: 10)
for departure in departures {
    // Only departures with real-time data have a delay.
    var delay = ""
    if let seconds = departure.delaySeconds {
        let minutes = seconds / 60
        delay = minutes < 0 ? "\(minutes)" : "+\(minutes)"
    }
    print(
        "  \(localTime(at: departure.stop, departure.scheduledTime)) \(delay)"
            + " \(departure.line.name) \(departure.direction ?? "?")"
            + (departure.cancelled ? " (cancelled)" : ""))
    // What the operator reports about the ride, such as a closed road.
    for remark in departure.remarks where remark.kind == .disruption {
        if let title = remark.title { print("      ! \(title)") }
    }
}

// The run of the first vehicle: every stop it calls at. The ID only works
// on the network it came from, and for about a day.
if let id = departures.compactMap(\.journeyId).first {
    let journey = try await client.journey(in: network, id: id)
    print("Stops of \(journey.line.name):")
    for stopover in journey.stops {
        // The first stop has no arrival and the last no departure.
        let time = (stopover.scheduledDeparture ?? stopover.scheduledArrival)
            .map { localTime(at: stopover.stop, $0) }
        print("  \(time ?? "??:??") \(stopover.stop.name)")
    }
}
