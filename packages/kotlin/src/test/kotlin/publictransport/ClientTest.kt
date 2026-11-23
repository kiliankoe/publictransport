package publictransport

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.Call
import okhttp3.EventListener
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.jupiter.api.DynamicTest
import org.junit.jupiter.api.DynamicTest.dynamicTest
import org.junit.jupiter.api.TestFactory
import java.io.File
import java.io.IOException
import java.time.Instant
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.reflect.KClass
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

// Building requests and parsing answers is the Rust core's job and tested
// there, so the tests compare with what the core itself returns.

private const val USER_AGENT = "publictransport-tests/1.0"

/** The day the fixtures were recorded. */
internal val time: Instant = Instant.parse("2026-11-16T13:24:00Z")

/** A stop that another network returned. */
internal val foreignStop =
    Location(
        id = "8010085",
        globalIds = listOf(GlobalId(IdScheme.IFOPT, "de:14612:28:3:2")),
        kind = LocationKind.STOP,
        name = "Dresden Hbf",
        locality = null,
        coordinate = null,
        timezone = null,
        network = "other",
    )
private val otherForeignStop =
    foreignStop.copy(
        id = "8010089",
        globalIds = listOf(GlobalId(IdScheme.IFOPT, "de:14612:13")),
        name = "Dresden Albertplatz",
    )
private val hauptbahnhof = LocationQuery(text = "Hauptbahnhof")
private val hbf = Place.Id("de:14612:28")
private val albertplatz = Place.Id("de:14612:13")

/** The reference to the trips after those of the trips fixture. */
private val later: String by lazy {
    checkNotNull(
        BundledNetwork.VVO_EFA
            .network()
            .parseTrips(ok(fixture("efa/vvo-trip.json")))
            .later,
    )
}

internal fun fixture(path: String): ByteArray = File("../../crates/core/tests/fixtures/$path").readBytes()

internal fun ok(body: ByteArray) = HttpResponse(status = 200u, headers = emptyList(), body = body)

private fun answer(
    body: ByteArray,
    status: Int = 200,
    headers: List<Header> = emptyList(),
) = MockResponse
    .Builder()
    .code(status)
    .apply { headers.forEach { addHeader(it.name, it.value) } }
    .body(Buffer().write(body))
    .build()

private fun efa(server: MockWebServer) =
    Network(
        id = "vvo",
        name = "VVO",
        endpoint = server.url("/std3").toString(),
        protocol = Protocol.EFA,
        timezone = "Europe/Berlin",
    )

// TRIAS is the protocol whose requests have a body and a header of their
// own, next to the one of the network. It also sends the limit of a search.
private fun trias(server: MockWebServer) =
    Network(
        id = "trias",
        name = "TRIAS",
        endpoint = server.url("/trias").toString(),
        protocol = Protocol.TRIAS,
        requestorRef = "OpenService",
        headers = listOf(Header("X-Api-Key", "secret")),
    )

/** Runs a test against a server that gives these answers in order. */
private fun serving(
    vararg answers: MockResponse,
    test: suspend (MockWebServer) -> Unit,
) = MockWebServer().use { server ->
    answers.forEach(server::enqueue)
    server.start()
    runBlocking { test(server) }
}

/** One call of the client, next to the request the core builds for it. */
private class Case(
    val name: String,
    val fixture: String,
    val network: (MockWebServer) -> Network = ::efa,
    val request: Network.() -> HttpRequest,
    val parse: Network.(HttpResponse) -> Any,
    val call: suspend Client.(Network) -> Any,
)

private fun locations(
    name: String,
    query: LocationQuery,
    fixture: String = "efa/vvo-stopfinder.json",
    network: (MockWebServer) -> Network = ::efa,
    call: suspend Client.(Network) -> List<Location>,
) = Case(name, fixture, network, { locationsRequest(query) }, { parseLocations(it) }, call)

private fun departures(
    name: String,
    query: DeparturesQuery,
    call: suspend Client.(Network) -> List<Departure>,
) = Case(name, "efa/vvo-departures.json", ::efa, { departuresRequest(query) }, { parseDepartures(it) }, call)

private fun arrivals(
    name: String,
    query: DeparturesQuery,
    call: suspend Client.(Network) -> List<Arrival>,
) = Case(name, "efa/vvo-arrivals.json", ::efa, { arrivalsRequest(query) }, { parseArrivals(it) }, call)

private fun trips(
    name: String,
    query: () -> TripsQuery,
    call: suspend Client.(Network) -> Trips,
) = Case(name, "efa/vvo-trip.json", ::efa, { tripsRequest(query()) }, { parseTrips(it) }, call)

private val calls =
    listOf(
        locations("locations by query", hauptbahnhof) { locations(it, hauptbahnhof) },
        locations(
            "locations by text, kinds and limit, with a body and headers",
            LocationQuery("Hauptbahnhof", 3u, listOf(LocationKind.STOP)),
            fixture = "trias/vvo-locations.xml",
            network = ::trias,
        ) { locations(it, "Hauptbahnhof", listOf(LocationKind.STOP), 3) },
        departures("departures by query", DeparturesQuery(hbf)) { departures(it, DeparturesQuery(hbf)) },
        departures("departures at an ID, with time and limit", DeparturesQuery(hbf, time, 7u)) {
            departures(it, "de:14612:28", time, 7)
        },
        departures(
            "departures at a location of another network",
            DeparturesQuery(Place.Location(foreignStop)),
        ) { departures(it, foreignStop) },
        arrivals("arrivals at an ID, with time and limit", DeparturesQuery(hbf, time, 7u)) {
            arrivals(it, "de:14612:28", time, 7)
        },
        trips("trips by query", { TripsQuery(hbf, albertplatz) }) { trips(it, TripsQuery(hbf, albertplatz)) },
        trips(
            "trips between locations, departing at a time, with limit",
            {
                TripsQuery(
                    Place.Location(foreignStop),
                    Place.Location(otherForeignStop),
                    TripTime.DepartAt(time),
                    3u,
                )
            },
        ) { trips(it, foreignStop, otherForeignStop, departAt = time, limit = 3) },
        trips(
            "trips from an ID to a coordinate, arriving by a time",
            { TripsQuery(hbf, Place.Coordinate(Coordinate(51.04, 13.73)), TripTime.ArriveBy(time)) },
        ) { trips(it, hbf, Coordinate(51.04, 13.73).asPlace(), arriveBy = time) },
        trips("trips of the next page", { TripsQuery(hbf, albertplatz, page = later) }) {
            trips(it, hbf, albertplatz, page = later)
        },
    )

class ClientTest {
    @TestFactory
    fun `sends the request of the core and returns what the core parses`(): List<DynamicTest> =
        calls.map { case ->
            dynamicTest(case.name) {
                val body = fixture(case.fixture)
                serving(answer(body)) { server ->
                    val network = case.network(server)
                    val request = case.request(network)

                    assertEquals(case.parse(network, ok(body)), case.call(Client(USER_AGENT), network))

                    assertEquals(1, server.requestCount)
                    val sent = server.takeRequest()
                    assertEquals(request.method.name, sent.method)
                    // Both sides go through OkHttp's parser, so that only a different URL fails.
                    assertEquals(request.url.toHttpUrl(), sent.url)
                    assertContentEquals(request.body ?: ByteArray(0), sent.body?.toByteArray() ?: ByteArray(0))
                    for (header in request.headers) {
                        assertEquals(header.value, sent.headers[header.name], header.name)
                    }
                    assertEquals(listOf(USER_AGENT), sent.headers.values("User-Agent"))
                }
            }
        }

    @Test
    fun `user agent of the network replaces the one of the client`() =
        serving(answer(fixture("efa/vvo-stopfinder.json"))) { server ->
            val network = efa(server).copy(headers = listOf(Header("User-Agent", "network/1.0")))

            Client(USER_AGENT).locations(network, hauptbahnhof)

            assertEquals(listOf("network/1.0"), server.takeRequest().headers.values("User-Agent"))
        }

    /** An answer the core refuses, which OkHttp would answer with a second request. */
    private class Refused(
        val name: String,
        val error: KClass<out PublicTransportException>,
        val status: Int,
        val headers: List<Header> = emptyList(),
        val httpClient: OkHttpClient? = null,
    )

    @TestFactory
    fun `error of the core comes out unchanged and the request is sent once`(): List<DynamicTest> {
        val moved = listOf(Header("Location", "/new"))
        return listOf(
            Refused("redirect", PublicTransportException.Moved::class, 302, moved),
            // A client of the caller follows redirects unless the library turns that off.
            Refused("redirect, client of the caller", PublicTransportException.Moved::class, 302, moved, OkHttpClient()),
            Refused("too many requests", PublicTransportException.RateLimited::class, 429),
            // OkHttp sends these two again by itself.
            Refused("request timeout", PublicTransportException.Server::class, 408),
            Refused(
                "unavailable, retry at once",
                PublicTransportException.Unavailable::class,
                503,
                listOf(Header("Retry-After", "0")),
            ),
        ).map { refused ->
            dynamicTest(refused.name) {
                // The second answer is what a followed redirect or a retry would get.
                serving(
                    answer(ByteArray(0), refused.status, refused.headers),
                    answer(fixture("efa/vvo-departures.json")),
                ) { server ->
                    val network = efa(server)
                    // Status and headers have to reach the core for the same error.
                    val expected =
                        assertFailsWith<PublicTransportException> {
                            network.parseDepartures(HttpResponse(refused.status.toUShort(), refused.headers, ByteArray(0)))
                        }
                    assertEquals(refused.error, expected::class)

                    val thrown =
                        assertFailsWith<PublicTransportException> {
                            Client(USER_AGENT, refused.httpClient).departures(network, "de:14612:28")
                        }

                    assertEquals(expected::class, thrown::class)
                    assertEquals(expected.toString(), thrown.toString())
                    assertEquals(1, server.requestCount)
                }
            }
        }
    }

    @Test
    fun `failed exchange is the exception of OkHttp`() {
        // Nothing listens on the port of a closed server.
        val network = MockWebServer().use { efa(it.apply { start() }) }

        val thrown = assertFailsWith<IOException> { runBlocking { Client(USER_AGENT).locations(network, hauptbahnhof) } }

        assertIs<java.net.ConnectException>(thrown)
    }

    @Test
    fun `answer above 16 MB is refused`() {
        val tooLarge = Buffer().write(ByteArray(17 * 1024 * 1024) { ' '.code.toByte() })
        // Without a `Content-Length`, as a hostile server would send it.
        serving(MockResponse.Builder().chunkedBody(tooLarge, 1024 * 1024).build()) { server ->
            assertFailsWith<ResponseTooLargeException> { Client(USER_AGENT).locations(efa(server), hauptbahnhof) }
        }
    }

    @Test
    fun `cancelling the coroutine cancels the request`() =
        serving(MockResponse.Builder().headersDelay(10, TimeUnit.SECONDS).build()) { server ->
            // The coroutine ends when it is cancelled either way, so the test asks OkHttp about its call.
            val cancelled = CountDownLatch(1)
            val listener =
                object : EventListener() {
                    override fun canceled(call: Call) = cancelled.countDown()
                }
            val client = Client(USER_AGENT, OkHttpClient.Builder().eventListener(listener).build())
            coroutineScope {
                var thrown: Throwable? = null
                val call =
                    launch(Dispatchers.IO) {
                        thrown = runCatching { client.locations(efa(server), hauptbahnhof) }.exceptionOrNull()
                    }
                assertNotNull(server.takeRequest(5, TimeUnit.SECONDS), "the request was not sent")

                call.cancelAndJoin()

                assertIs<CancellationException>(thrown)
                assertTrue(cancelled.await(5, TimeUnit.SECONDS), "the request goes on")
            }
        }

    @TestFactory
    fun `invalid plain call is refused before a request`(): List<DynamicTest> =
        listOf<Pair<String, suspend Client.(Network) -> Any>>(
            "trips between places with both times" to { trips(it, hbf, albertplatz, departAt = time, arriveBy = time) },
            "trips between locations with both times" to {
                trips(it, foreignStop, otherForeignStop, departAt = time, arriveBy = time)
            },
            "trips with a negative limit" to { trips(it, hbf, albertplatz, limit = -1) },
            "departures with a negative limit" to { departures(it, "de:14612:28", limit = -1) },
            // The core leaves the limit out of an EFA search, so it cannot refuse this one.
            "locations with a negative limit" to { locations(it, "Hauptbahnhof", limit = -1) },
        ).map { (name, call) ->
            dynamicTest(name) {
                // With an answer, a call that is sent fails here and not at a timeout.
                serving(answer(ByteArray(0))) { server ->
                    assertFailsWith<IllegalArgumentException> { Client(USER_AGENT).call(efa(server)) }

                    assertEquals(0, server.requestCount)
                }
            }
        }

    @Test
    fun `user agent that no request can carry is refused`() {
        for (userAgent in listOf("", " \t", "mein-äpp/1.0")) {
            assertFailsWith<IllegalArgumentException>(userAgent) { Client(userAgent) }
        }
    }
}
