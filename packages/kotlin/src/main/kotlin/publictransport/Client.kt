package publictransport

import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Dispatcher
import okhttp3.Headers
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okio.Buffer
import java.io.IOException
import java.time.Duration
import java.time.Instant
import java.util.concurrent.SynchronousQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

// Real answers stay below 1 MB. The limit keeps a broken or hostile server
// from filling the memory.
private const val LARGEST_BODY = 16L * 1024 * 1024

/**
 * The answer was larger than 16 MB, more than any real one, so reading it
 * was stopped.
 */
class ResponseTooLargeException : IOException("the response is larger than 16 MB")

/** This location as an end of a trip, for [Client.trips] with a coordinate or an ID at the other end. */
fun Location.asPlace(): Place = Place.Location(this)

/** This position as an end of a trip, for [Client.trips]. */
fun Coordinate.asPlace(): Place = Place.Coordinate(this)

/**
 * Sends requests to public transport servers and waits for the answers.
 *
 * One client serves every [Network] and shares its connections between
 * them, so create one and keep it. It does not retry, limit its request
 * rate or cache; those are decisions for the application.
 *
 * ```kotlin
 * val client = Client("my-app/1.0 (+https://example.org)")
 * val network = BundledNetwork.VVO_EFA.network()
 * val stops = client.locations(network, "Hauptbahnhof")
 * val board = client.departures(network, stops.first())
 * ```
 *
 * Every call throws a [PublicTransportException] for what the server or the
 * query got wrong, such as [PublicTransportException.NotFound], and an
 * [IOException] if the HTTP exchange failed. The client does not follow
 * redirects: a moved server is a [PublicTransportException.Moved]. A
 * cancelled coroutine cancels its request. A network whose endpoint is no
 * HTTP address, or with a header that HTTP cannot carry, is an
 * [IllegalArgumentException].
 *
 * @param userAgent Sent with every request. It should name the application,
 *   its version and a way to contact its authors, e.g.
 *   `my-app/1.0 (+https://example.org)`. Some servers require that.
 * @param httpClient For a caller who configures OkHttp, e.g. with a proxy
 *   or an interceptor. The client uses its connections and settings, except
 *   that it never follows a redirect and never sends a request again.
 *   Without it, a request fails when the server sends nothing for 30
 *   seconds.
 * @throws IllegalArgumentException if the user agent is blank or holds a
 *   character that an HTTP header cannot carry, such as one outside ASCII.
 */
class Client(
    private val userAgent: String,
    httpClient: OkHttpClient? = null,
) {
    init {
        require(userAgent.isNotBlank()) { "the user agent names the application to the servers, so it cannot be blank" }
        // Throws here what every request would throw otherwise.
        Headers.headersOf("User-Agent", userAgent)
    }

    // The core gets the first answer of the server, whatever it is: it
    // reports a redirect as a moved endpoint, and the application decides
    // about a second try.
    private val http: OkHttpClient =
        (httpClient?.newBuilder() ?: ownClient())
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(false)
            // OkHttp has no setting for the request it sends again after a 503 with this header.
            .addNetworkInterceptor { chain ->
                chain
                    .proceed(chain.request())
                    .newBuilder()
                    .removeHeader("Retry-After")
                    .build()
            }.build()

    /**
     * Searches for locations by name. The best match comes first.
     *
     * @param text What to search for, as a user typed it.
     * @param kinds The kinds of location wanted. Empty asks for every kind.
     * @param limit The largest number wanted. EFA servers get no limit and
     *   return every match.
     * @throws IllegalArgumentException if the limit is negative.
     */
    suspend fun locations(
        network: Network,
        text: String,
        kinds: List<LocationKind> = emptyList(),
        limit: Int? = null,
    ): List<Location> = locations(network, LocationQuery(text, count(limit), kinds))

    /** Searches for locations with a query of the core library. */
    suspend fun locations(
        network: Network,
        query: LocationQuery,
    ): List<Location> = network.parseLocations(send(network.locationsRequest(query)))

    /**
     * Finds the stops near a position. The nearest comes first, and
     * [Coordinate.distanceMeters] gives how far one is.
     *
     * @param position Where to look. A position off the globe throws
     *   [PublicTransportException.InvalidQuery].
     * @param radiusMeters How far from the position to look. 1000 m if left
     *   out. MOTIS has no radius and returns the nearest stops however far
     *   they are.
     * @param limit The largest number wanted. Servers treat it as a guide.
     * @throws IllegalArgumentException if the radius or the limit is
     *   negative.
     */
    suspend fun nearby(
        network: Network,
        position: Coordinate,
        radiusMeters: Int? = null,
        limit: Int? = null,
    ): List<Location> {
        require(radiusMeters == null || radiusMeters >= 0) { "the radius $radiusMeters is no distance" }
        return nearby(network, NearbyQuery(position, radiusMeters?.toUInt(), count(limit)))
    }

    /** Finds the stops near a position with a query of the core library. */
    suspend fun nearby(
        network: Network,
        query: NearbyQuery,
    ): List<Location> = network.parseNearby(send(network.nearbyRequest(query)))

    /**
     * Lists the next departures at a stop, ordered by expected time.
     *
     * @param stop A location from a search, a departure or a trip. A stop
     *   that another network returned works where this network can name it,
     *   which EFA, TRIAS and OJP can for stops with a global ID; otherwise
     *   the call throws [PublicTransportException.InvalidQuery].
     * @param time The time from which on departures are wanted. Now if left
     *   out.
     * @param limit How many departures are wanted. This is not a maximum:
     *   servers treat it as a guide and may return more or fewer.
     * @throws IllegalArgumentException if the limit is negative.
     */
    suspend fun departures(
        network: Network,
        stop: Location,
        time: Instant? = null,
        limit: Int? = null,
    ): List<Departure> = departures(network, DeparturesQuery(Place.Location(stop), time, count(limit)))

    /**
     * Lists the next departures at the stop with this ID on the network,
     * as an application stored it. The other arguments are those of the
     * call with a [Location].
     */
    suspend fun departures(
        network: Network,
        stopId: String,
        time: Instant? = null,
        limit: Int? = null,
    ): List<Departure> = departures(network, DeparturesQuery(Place.Id(stopId), time, count(limit)))

    /** Lists departures with a query of the core library. */
    suspend fun departures(
        network: Network,
        query: DeparturesQuery,
    ): List<Departure> = network.parseDepartures(send(network.departuresRequest(query)))

    /**
     * Lists the next arrivals at a stop, ordered by expected time. The
     * arguments are those of [departures]. An arrival has an
     * [origin][Arrival.origin], where the vehicle comes from, in place of
     * the direction.
     *
     * @throws IllegalArgumentException if the limit is negative.
     */
    suspend fun arrivals(
        network: Network,
        stop: Location,
        time: Instant? = null,
        limit: Int? = null,
    ): List<Arrival> = arrivals(network, DeparturesQuery(Place.Location(stop), time, count(limit)))

    /**
     * Lists the next arrivals at the stop with this ID on the network, as
     * an application stored it. The other arguments are those of the call
     * with a [Location].
     */
    suspend fun arrivals(
        network: Network,
        stopId: String,
        time: Instant? = null,
        limit: Int? = null,
    ): List<Arrival> = arrivals(network, DeparturesQuery(Place.Id(stopId), time, count(limit)))

    /** Lists arrivals with a query of the core library, the one of departures. */
    suspend fun arrivals(
        network: Network,
        query: DeparturesQuery,
    ): List<Arrival> = network.parseArrivals(send(network.arrivalsRequest(query)))

    /**
     * Fetches the run of one vehicle: all its stops with times and
     * platforms.
     *
     * @param id The [journeyId][Departure.journeyId] of a departure, an
     *   arrival or a leg that this network returned. It has no meaning on
     *   another network, and a server knows a run for about a day: after
     *   that the call throws [PublicTransportException.NotFound].
     */
    suspend fun journey(
        network: Network,
        id: String,
    ): Journey = network.parseJourney(send(network.journeyRequest(id)))

    /**
     * Plans trips between two locations, in the order the server ranks
     * them. A location that another network returned works as well.
     *
     * @param departAt Leave at this time or later. Now if both times are
     *   left out.
     * @param arriveBy Arrive at this time or earlier.
     * @param limit How many trips are wanted. This is not a maximum: servers
     *   treat it as a guide and may return more or fewer.
     * @param page [Trips.earlier] or [Trips.later] of an answer, to continue
     *   that search with the same other arguments. A server that searches a
     *   window of time can answer with no trips and a reference to later
     *   ones, as at night: follow it before telling a user that there are
     *   none.
     * @throws IllegalArgumentException if both times are given or the limit
     *   is negative.
     */
    suspend fun trips(
        network: Network,
        origin: Location,
        destination: Location,
        departAt: Instant? = null,
        arriveBy: Instant? = null,
        limit: Int? = null,
        page: String? = null,
    ): Trips = trips(network, Place.Location(origin), Place.Location(destination), departAt, arriveBy, limit, page)

    /**
     * Plans trips between two places: each is a location or a coordinate,
     * made a place with `asPlace()`, or a [Place.Id] on this network, as in
     * `trips(network, stop.asPlace(), Coordinate(51.04, 13.73).asPlace())`.
     * The other arguments are those of the call with two locations.
     */
    suspend fun trips(
        network: Network,
        origin: Place,
        destination: Place,
        departAt: Instant? = null,
        arriveBy: Instant? = null,
        limit: Int? = null,
        page: String? = null,
    ): Trips {
        require(departAt == null || arriveBy == null) {
            "a trip is bound to its departure or to its arrival, not to both"
        }
        val time = departAt?.let(TripTime::DepartAt) ?: arriveBy?.let(TripTime::ArriveBy)
        return trips(network, TripsQuery(origin, destination, time, count(limit), page))
    }

    /** Plans trips with a query of the core library. */
    suspend fun trips(
        network: Network,
        query: TripsQuery,
    ): Trips = network.parseTrips(send(network.tripsRequest(query)))

    /** A number of results as the core library takes it. */
    private fun count(limit: Int?): UInt? {
        require(limit == null || limit >= 0) { "the limit $limit is no number of results" }
        return limit?.toUInt()
    }

    private suspend fun send(request: HttpRequest): HttpResponse {
        val builder = Request.Builder().url(request.url).header("User-Agent", userAgent)
        // After the user agent, so that one from the network takes its place.
        request.headers.forEach { builder.header(it.name, it.value) }
        // The body has no content type of its own, so OkHttp sends the one of the core unchanged.
        builder.method(request.method.name, request.body?.toRequestBody())
        val call = http.newCall(builder.build())

        return suspendCancellableCoroutine { continuation ->
            continuation.invokeOnCancellation { call.cancel() }
            call.enqueue(
                object : Callback {
                    override fun onFailure(
                        call: Call,
                        e: IOException,
                    ) = continuation.resumeWithException(e)

                    // The body is read here, on OkHttp's thread, so that no thread of the caller waits for it.
                    override fun onResponse(
                        call: Call,
                        response: Response,
                    ) = runCatching { response.use(::read) }.fold(continuation::resume, continuation::resumeWithException)
                },
            )
        }
    }
}

/** Reads the answer, up to the largest size. An answer of any status goes to the core. */
private fun read(response: Response): HttpResponse {
    val body = Buffer()
    val source = response.body.source()
    while (source.read(body, 64 * 1024) != -1L) {
        if (body.size > LARGEST_BODY) throw ResponseTooLargeException()
    }
    val headers = response.headers.map { (name, value) -> Header(name, value) }
    return HttpResponse(response.code.toUShort(), headers, body.readByteArray())
}

/** The settings of a client that the caller did not configure. */
private fun ownClient(): OkHttpClient.Builder {
    val timeout = Duration.ofSeconds(30)
    // OkHttp's own threads would keep the JVM alive for a minute after the last request.
    val threads =
        ThreadPoolExecutor(0, Int.MAX_VALUE, 60, TimeUnit.SECONDS, SynchronousQueue()) { task ->
            Thread(task, "publictransport").apply { isDaemon = true }
        }
    return OkHttpClient
        .Builder()
        .dispatcher(Dispatcher(threads))
        .connectTimeout(timeout)
        .readTimeout(timeout)
        .writeTimeout(timeout)
}
