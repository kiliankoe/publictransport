// Searches for places by name on the EFA server of VVO, the transport
// association of Dresden.

import publictransport.BundledNetwork
import publictransport.Client
import java.util.Locale

suspend fun main() {
    // Servers want to know who asks: name your application and a way to reach you.
    val client = Client("publictransport-example (+https://github.com/kiliankoe/publictransport)")
    val network = BundledNetwork.VVO_EFA.network()

    // Every kind of place. `kinds = listOf(LocationKind.STOP)` asks for stops only.
    val locations = client.locations(network, "Dresden Zwinger")

    // The best match comes first.
    for (location in locations.take(5)) {
        val position =
            location.coordinate?.let { " at %.4f, %.4f".format(Locale.ROOT, it.latitude, it.longitude) } ?: ""
        println("${location.kind}: ${location.name} (${location.id})$position")
    }
}
