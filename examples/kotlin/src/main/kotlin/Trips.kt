// Plans trips between two stops on the OTP server of Entur, the journey
// planner for Norway, and asks for the trips after the first ones.

import publictransport.BundledNetwork
import publictransport.Client
import publictransport.Header
import publictransport.LegEnd
import publictransport.LegKind
import publictransport.LocationKind
import publictransport.Trip
import java.time.format.DateTimeFormatter

private val clock = DateTimeFormatter.ofPattern("HH:mm")

// The expected time of day at the stop: the real-time one if there is one.
// Times are `Instant` values, and a location gives the time on the clocks
// there, where the server names its time zone.
private fun localTime(end: LegEnd): String = end.stop.localTime(end.expectedTime)?.format(clock) ?: "??:??"

private fun printTrip(trip: Trip) {
    val departure = trip.departure ?: return
    val arrival = trip.arrival ?: return
    println("  ${localTime(departure)} to ${localTime(arrival)}, ${trip.transfers} change(s)")
    for (leg in trip.legs) {
        val how =
            when (val kind = leg.kind) {
                is LegKind.Transit -> "${kind.line.name} to ${kind.direction ?: "?"}"
                is LegKind.Walk -> "walk"
                else -> "other"
            }
        println(
            "    ${localTime(leg.departure)} ${leg.departure.stop.name}" +
                " -> ${localTime(leg.arrival)} ${leg.arrival.stop.name}: $how",
        )
    }
}

suspend fun main() {
    // Servers want to know who asks: name your application and a way to reach you.
    val client = Client("publictransport-example (+https://github.com/kiliankoe/publictransport)")
    val entur = BundledNetwork.ENTUR.network()
    // Entur asks every client for its name, as `company-application`.
    val network = entur.copy(headers = entur.headers + Header("ET-Client-Name", "publictransport-example"))

    val origin = client.locations(network, "Oslo S", kinds = listOf(LocationKind.STOP)).first()
    val destination = client.locations(network, "Lillehammer stasjon", kinds = listOf(LocationKind.STOP)).first()

    // Leaving now. `departAt =` and `arriveBy =` ask for another time.
    // An end can also be a coordinate, with `asPlace()` on both ends, or the
    // ID of a stop as a `Place.Id`.
    val found = client.trips(network, origin, destination, limit = 3)
    println("Trips from ${origin.name} to ${destination.name}:")
    found.trips.forEach(::printTrip)

    // The same arguments again, with the reference to the trips after these.
    found.later?.let { later ->
        println("Later trips:")
        val more = client.trips(network, origin, destination, limit = 3, page = later)
        more.trips.forEach(::printTrip)
    }
}
