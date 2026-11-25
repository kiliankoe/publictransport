package publictransport

import java.time.Instant
import java.time.OffsetDateTime
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue

class BindingsTest {
    @Test
    fun `local time has the offset that holds at the location`() {
        val berlin = foreignStop.copy(timezone = "Europe/Berlin")

        // `OffsetDateTime`s are equal only with the same offset, and summer has another one.
        assertEquals(OffsetDateTime.parse("2026-11-16T14:24:00+01:00"), berlin.localTime(time))
        assertEquals(OffsetDateTime.parse("2026-07-16T15:24:00+02:00"), berlin.localTime(Instant.parse("2026-07-16T13:24:00Z")))
        assertNull(foreignStop.localTime(time))
        assertNull(foreignStop.copy(timezone = "Mars/Olympus").localTime(time))
    }

    @Test
    fun `message of an exception is the sentence of the core`() {
        val tooMany = HttpResponse(429u, listOf(Header("Retry-After", "120")), ByteArray(0))

        val thrown =
            assertFailsWith<PublicTransportException.RateLimited> {
                BundledNetwork.VVO_EFA.network().parseDepartures(DeparturesQuery(Place.Id("a")), tooMany)
            }

        assertEquals("the server received too many requests", thrown.message)
        assertEquals(120u, thrown.retryAfterSeconds)
    }

    // The values come from the core, so this checks that each one is a
    // property and crosses the boundary, not how the core derives it.
    @Test
    fun `derived values are properties`() {
        val network = BundledNetwork.VVO_EFA.network()
        val departure = network.parseDepartures(DeparturesQuery(Place.Id("a")), ok(fixture("efa/vvo-departures.json"))).first()
        val trip = network.parseTrips(TripsQuery(Place.Id("a"), Place.Id("b")), ok(fixture("efa/vvo-trip.json"))).trips.first()
        val start = trip.legs.first().departure

        assertEquals(departure.realtimeTime ?: departure.scheduledTime, departure.expectedTime)
        assertEquals(start.realtimeTime ?: start.scheduledTime, start.expectedTime)
        assertEquals(start, trip.departure)
        assertEquals(trip.legs.last().arrival, trip.arrival)
        assertTrue(trip.transfers < trip.legs.size.toUInt())
        assertEquals(OffsetDateTime.parse("2026-11-16T14:24:00+01:00"), departure.localScheduledTime)
        assertEquals(OffsetDateTime.parse("2026-11-16T14:25:00+01:00"), departure.localExpectedTime)
        assertEquals(60L, start.delaySeconds)
        assertEquals("14", start.platform)
        assertEquals(840L, trip.durationSeconds)
        assertEquals(listOf(GlobalId(IdScheme.IFOPT, "de:14612:28")), foreignStop.stationIds)
        assertEquals(network.id, BundledNetwork.VVO_EFA.id)
    }

    @Test
    fun `distance between positions crosses the boundary`() {
        assertEquals(111_200.0, Coordinate(51.0, 13.7).distanceMeters(Coordinate(52.0, 13.7)), 100.0)
    }

    @Test
    fun `remarks cross the boundary`() {
        val departures =
            BundledNetwork.VRR_EFA
                .network()
                .parseDepartures(DeparturesQuery(Place.Id("a")), ok(fixture("efa/vrr-departures.json")))

        val remark = departures.flatMap { it.remarks }.first { it.id == "314433" }
        assertEquals(RemarkKind.DISRUPTION, remark.kind)
        assertTrue(remark.title.orEmpty().startsWith("Fahrleitungsarbeiten in Heerdt"))
        assertEquals(Instant.parse("2026-11-16T07:00:00Z"), remark.validFrom)
    }
}
