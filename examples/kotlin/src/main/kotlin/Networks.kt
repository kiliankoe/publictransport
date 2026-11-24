// Lists the bundled networks that cover a position. This sends no request:
// the networks and their areas ship with the package.

import publictransport.Coordinate
import publictransport.Registry

fun main() {
    val registry = Registry.bundled()
    println("${registry.networks().size} networks are in the registry.")

    val duesseldorf = Coordinate(latitude = 51.2198, longitude = 6.7943)
    // The best coverage comes first: real-time data, then complete timetables,
    // then any data.
    println("At Düsseldorf Hauptbahnhof:")
    for (covering in registry.covering(duesseldorf)) {
        val network = covering.network
        println("  ${covering.tier}: ${network.name} (${network.id}, ${network.protocol})")
    }

    // A network by its ID, as an application would store it.
    registry.get("vrr-efa")?.let { network ->
        println("vrr-efa is ${network.name} at ${network.endpoint}")
    }
}
