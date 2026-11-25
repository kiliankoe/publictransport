// Plans trips between two stops on the OTP server of Entur, the journey
// planner for Norway, and asks for the trips after the first ones.

import Foundation
import PublicTransport

// Servers want to know who asks: name your application and a way to reach you.
let client = Client(userAgent: "publictransport-example (+https://github.com/kiliankoe/publictransport)")
var network = TransitNetwork.bundled(.entur)
// Entur asks every client for its name, as `company-application`.
network.headers.append(Header(name: "ET-Client-Name", value: "publictransport-example"))

/// The expected time of day at the stop: the real-time one if there is
/// one. Times are `Date` values, and a location has the time zone to show
/// them in.
func localTime(_ end: LegEnd) -> String {
    end.expectedTime.formatted(Date.FormatStyle(time: .shortened, timeZone: end.stop.timeZone ?? .gmt))
}

func printTrip(_ trip: Trip) {
    print(
        "  \(localTime(trip.departure)) to \(localTime(trip.arrival)),"
            + " \(trip.durationSeconds / 60) min, \(trip.transfers) change(s)")
    for leg in trip.legs {
        // The vehicle of the ride before goes on under this line.
        let seated = leg.sameVehicle ? ", stay seated" : ""
        let how =
            switch leg.kind {
            case .transit(let line, let direction): "\(line.name) to \(direction ?? "?")\(seated)"
            case .walk: "walk"
            default: "other"
            }
        print(
            "    \(localTime(leg.departure)) \(leg.departure.stop.name)"
                + " -> \(localTime(leg.arrival)) \(leg.arrival.stop.name): \(how)")
    }
}

let origins = try await client.locations(in: network, matching: "Oslo S", kinds: [.stop])
let destinations = try await client.locations(in: network, matching: "Lillehammer stasjon", kinds: [.stop])
guard let origin = origins.first, let destination = destinations.first else { fatalError("found no stop") }

// Leaving now. `departingAt:` and `arrivingBy:` ask for another time.
// An end can also be a coordinate or the ID of a stop.
let found = try await client.trips(in: network, from: origin, to: destination, limit: 3)
print("Trips from \(origin.name) to \(destination.name):")
found.trips.forEach(printTrip)

// The same arguments again, with the reference to the trips after these.
if let later = found.later {
    print("Later trips:")
    let more = try await client.trips(in: network, from: origin, to: destination, limit: 3, page: later)
    more.trips.forEach(printTrip)
}
