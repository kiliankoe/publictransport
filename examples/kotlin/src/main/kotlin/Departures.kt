// Lists the next departures at a stop on the HAFAS server of VBB, the
// transport association of Berlin and Brandenburg.

import publictransport.BundledNetwork
import publictransport.Client
import publictransport.LocationKind
import java.time.Duration
import java.time.format.DateTimeFormatter

private val clock = DateTimeFormatter.ofPattern("HH:mm")

suspend fun main() {
    // Servers want to know who asks: name your application and a way to reach you.
    val client = Client("publictransport-example (+https://github.com/kiliankoe/publictransport)")
    val network = BundledNetwork.VBB_HAFAS.network()

    val stop = client.locations(network, "S+U Alexanderplatz", kinds = listOf(LocationKind.STOP)).first()

    println("Departures at ${stop.name}:")
    // From now on. `time =` asks for another time.
    for (departure in client.departures(network, stop, limit = 10)) {
        // Times are `Instant` values, and a location gives the time on the
        // clocks there, where the server names its time zone.
        val time = departure.stop.localTime(departure.scheduledTime)?.format(clock) ?: "??:??"
        // Only departures with real-time data have a delay.
        val delay =
            departure.realtimeTime?.let { "%+d".format(Duration.between(departure.scheduledTime, it).toMinutes()) }
        println(
            "  $time ${(delay ?: "").padStart(3)} ${departure.line.name.padEnd(5)} ${departure.direction ?: "?"}" +
                if (departure.cancelled) " (cancelled)" else "",
        )
    }
}
