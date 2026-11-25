import Foundation

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif
#if canImport(CoreLocation)
    import CoreLocation
#endif

/// Anything that names a place in a query: a ``Location`` from a search, a
/// departure or a trip, a ``Coordinate``, a ``Place``, or a `String` with
/// an ID of the network that is asked.
public protocol PlaceConvertible: Sendable {
    /// The value as a place.
    var place: Place { get }
}

extension Place: PlaceConvertible {
    public var place: Place { self }
}

extension Location: PlaceConvertible {
    public var place: Place { .location(self) }
}

extension Coordinate: PlaceConvertible {
    public var place: Place { .coordinate(self) }
}

extension String: PlaceConvertible {
    public var place: Place { .id(self) }
}

#if canImport(CoreLocation)
    extension CLLocationCoordinate2D: PlaceConvertible {
        public var place: Place { .coordinate(Coordinate(self)) }
    }

    extension Coordinate {
        /// The position of Core Location as one of this library.
        public init(_ coordinate: CLLocationCoordinate2D) {
            self.init(latitude: coordinate.latitude, longitude: coordinate.longitude)
        }
    }

    extension Registry {
        /// The networks for a position of Core Location, as for a
        /// ``Coordinate``.
        public func covering(at position: CLLocationCoordinate2D) -> [CoveringNetwork] {
            covering(at: Coordinate(position))
        }
    }

    extension Client {
        /// The stops near a position of Core Location, as for a
        /// ``Coordinate``.
        public func nearby(
            in network: TransitNetwork,
            at position: CLLocationCoordinate2D,
            radiusMeters: Int? = nil,
            limit: Int? = nil
        ) async throws -> [Location] {
            try await nearby(in: network, at: Coordinate(position), radiusMeters: radiusMeters, limit: limit)
        }
    }

    extension CLLocationCoordinate2D {
        /// The position of this library as one of Core Location.
        public init(_ coordinate: Coordinate) {
            self.init(latitude: coordinate.latitude, longitude: coordinate.longitude)
        }
    }
#endif

extension TransitNetwork {
    /// The network that ships with the library under this name, such as
    /// `TransitNetwork.bundled(.vvoEfa)`.
    public static func bundled(_ network: BundledNetwork) -> TransitNetwork {
        network.network()
    }

    /// Reads a network from an entry in the JSON format of the
    /// transport-apis repository.
    ///
    /// - Parameters:
    ///   - id: The name of the entry's file without `.json`.
    ///   - json: The entry.
    /// - Throws: ``PublicTransportError/invalidNetwork(message:)`` if the
    ///   text is no such entry or names a protocol the library lacks.
    public init(id: String, json: String) throws {
        self = try networkFromJson(id: id, json: json)
    }
}

extension Location {
    /// The time zone the location is in, for showing times there. `nil` if
    /// neither the server nor the network names one, or if the system
    /// does not know the name.
    public var timeZone: TimeZone? {
        timeZoneIdentifier.flatMap(TimeZone.init(identifier:))
    }
}

extension TransitNetwork {
    /// The time zone the server works in. `nil` if the network names none,
    /// or if the system does not know the name.
    public var timeZone: TimeZone? {
        timeZoneIdentifier.flatMap(TimeZone.init(identifier:))
    }
}

// Both have an `id` already, which lists in SwiftUI ask for.
extension Location: Identifiable {}
extension TransitNetwork: Identifiable {}

/// What can go wrong in the client itself. Failures that the server or the
/// query cause are a ``PublicTransportError``, and failures of the HTTP
/// exchange a `URLError`.
public enum ClientError: Error, Equatable, LocalizedError {
    /// The answer was larger than 16 MB, more than any real one, so it was
    /// not handed on.
    case responseTooLarge

    public var errorDescription: String? {
        switch self {
        case .responseTooLarge: "the response is larger than 16 MB"
        }
    }
}

/// Sends requests to public transport servers and waits for the answers.
///
/// One client serves every ``TransitNetwork``. It does not retry, limit its
/// request rate or cache; those are decisions for the application.
///
/// ```swift
/// let client = Client(userAgent: "my-app/1.0 (+https://example.org)")
/// let network = TransitNetwork.bundled(.vvoEfa)
/// let stops = try await client.locations(in: network, matching: "Hauptbahnhof")
/// let board = try await client.departures(in: network, at: stops[0])
/// ```
public struct Client: Sendable {
    /// Real answers stay below 1 MB.
    static let largestBody = 16 * 1024 * 1024

    private let userAgent: String
    private let session: URLSession

    /// Creates a client.
    ///
    /// - Parameters:
    ///   - userAgent: Sent with every request. It should name the
    ///     application, its version and a way to contact its authors, e.g.
    ///     `my-app/1.0 (+https://example.org)`. Some servers require that.
    ///     It must not be empty.
    ///   - session: For a caller who configures `URLSession`, e.g. with a
    ///     proxy. Without it, a request fails when the server sends nothing
    ///     for 30 seconds. On Linux such a session has to refuse redirects
    ///     itself, in its delegate, so that a moved server is reported as
    ///     ``PublicTransportError/moved(location:)``; on Apple platforms
    ///     the client sees to that.
    public init(userAgent: String, session: URLSession? = nil) {
        precondition(
            !userAgent.trimmingCharacters(in: .whitespaces).isEmpty,
            "the user agent names the application to the servers, so it cannot be empty")
        self.userAgent = userAgent
        if let session {
            self.session = session
        } else {
            let configuration = URLSessionConfiguration.ephemeral
            configuration.timeoutIntervalForRequest = 30
            // On Linux only the delegate of the session is asked about a
            // redirect, not the one of a request.
            self.session = URLSession(
                configuration: configuration, delegate: Unredirected(), delegateQueue: nil)
        }
    }

    /// Searches for locations by name. The best match comes first.
    ///
    /// - Parameters:
    ///   - text: What to search for, as a user typed it.
    ///   - kinds: The kinds of location wanted. Empty asks for every kind.
    ///   - limit: The largest number wanted.
    /// - Throws: ``PublicTransportError`` for what the server or the query
    ///   got wrong, `URLError` if the request got no answer, and
    ///   `CancellationError` if the task was cancelled.
    public func locations(
        in network: TransitNetwork,
        matching text: String,
        kinds: [LocationKind] = [],
        limit: Int? = nil
    ) async throws -> [Location] {
        let query = LocationQuery(text: text, limit: try count(limit), kinds: kinds)
        return try await locations(in: network, query: query)
    }

    /// Searches for locations with a query of the core library.
    public func locations(in network: TransitNetwork, query: LocationQuery) async throws -> [Location] {
        let response = try await send(network.locationsRequest(query: query))
        return try network.parseLocations(query: query, response: response)
    }

    /// Finds the stops near a position. The nearest comes first, and
    /// ``Coordinate/distanceMeters(to:)`` gives how far one is.
    ///
    /// - Parameters:
    ///   - position: Where to look.
    ///   - radiusMeters: How far from the position to look. 1000 m if left
    ///     out. Stops farther away are not returned. MOTIS takes no radius
    ///     and returns only the nearest stops, so stops in the circle can
    ///     be missing there.
    ///   - limit: The largest number wanted.
    /// - Throws: ``PublicTransportError``, such as `invalidQuery` for a
    ///   position off the globe, and `URLError` if the request got no
    ///   answer.
    public func nearby(
        in network: TransitNetwork,
        at position: Coordinate,
        radiusMeters: Int? = nil,
        limit: Int? = nil
    ) async throws -> [Location] {
        let radius = try radiusMeters.map { meters in
            guard let radius = UInt32(exactly: meters) else {
                throw PublicTransportError.invalidQuery(message: "the radius \(meters) is no distance")
            }
            return radius
        }
        let query = NearbyQuery(position: position, radiusMeters: radius, limit: try count(limit))
        return try await nearby(in: network, query: query)
    }

    /// Finds the stops near a position with a query of the core library.
    public func nearby(in network: TransitNetwork, query: NearbyQuery) async throws -> [Location] {
        let response = try await send(network.nearbyRequest(query: query))
        return try network.parseNearby(query: query, response: response)
    }

    /// Lists the next departures at a stop, ordered by expected time.
    ///
    /// - Parameters:
    ///   - stop: A location, or the ID of a stop on this network. A stop
    ///     that another network returned works where this network can name
    ///     it, which EFA, TRIAS and OJP can for stops with a global ID.
    ///   - time: The time from which on departures are wanted. Now if
    ///     left out. Departures expected before a given time are left out.
    ///   - limit: The largest number of departures wanted.
    /// - Throws: ``PublicTransportError``, such as `notFound` for a stop
    ///   the network does not know, and `URLError` if the request got no
    ///   answer.
    public func departures(
        in network: TransitNetwork,
        at stop: some PlaceConvertible,
        after time: Date? = nil,
        limit: Int? = nil
    ) async throws -> [Departure] {
        let query = DeparturesQuery(stop: stop.place, time: time, limit: try count(limit))
        return try await departures(in: network, query: query)
    }

    /// Lists departures with a query of the core library.
    public func departures(in network: TransitNetwork, query: DeparturesQuery) async throws -> [Departure] {
        let response = try await send(network.departuresRequest(query: query))
        return try network.parseDepartures(query: query, response: response)
    }

    /// Lists the next arrivals at a stop, ordered by expected time. An
    /// arrival has an ``Arrival/origin``, where the vehicle comes from, in
    /// place of the direction.
    ///
    /// - Parameters:
    ///   - stop: A location, or the ID of a stop on this network, as for
    ///     ``departures(in:at:after:limit:)``.
    ///   - time: The time from which on arrivals are wanted. Now if left
    ///     out. Arrivals expected before a given time are left out.
    ///   - limit: The largest number of arrivals wanted.
    /// - Throws: ``PublicTransportError``, such as `notFound` for a stop
    ///   the network does not know, and `URLError` if the request got no
    ///   answer.
    public func arrivals(
        in network: TransitNetwork,
        at stop: some PlaceConvertible,
        after time: Date? = nil,
        limit: Int? = nil
    ) async throws -> [Arrival] {
        let query = DeparturesQuery(stop: stop.place, time: time, limit: try count(limit))
        return try await arrivals(in: network, query: query)
    }

    /// Lists arrivals with a query of the core library, the one of
    /// departures.
    public func arrivals(in network: TransitNetwork, query: DeparturesQuery) async throws -> [Arrival] {
        let response = try await send(network.arrivalsRequest(query: query))
        return try network.parseArrivals(query: query, response: response)
    }

    /// Fetches the run of one vehicle: all its stops with times and
    /// platforms.
    ///
    /// - Parameters:
    ///   - id: The ``Departure/journeyId`` of a departure, an arrival or a
    ///     leg that this network returned. It has no meaning on another
    ///     network, and a server knows a run for about a day.
    /// - Throws: ``PublicTransportError``, such as `notFound` for a run
    ///   the server no longer knows, and `URLError` if the request got no
    ///   answer.
    public func journey(in network: TransitNetwork, id: String) async throws -> Journey {
        let response = try await send(network.journeyRequest(id: id))
        return try network.parseJourney(response: response)
    }

    /// Plans trips between two places, in the order the server ranks them.
    ///
    /// Each end is a location, also one that another network returned, a
    /// coordinate, or an ID on this network.
    ///
    /// - Parameters:
    ///   - departingAt: Leave at this time or later. Now if both times are
    ///     left out.
    ///   - arrivingBy: Arrive at this time or earlier. Only one of the two
    ///     times can be given.
    ///   - limit: How many trips are wanted. This is not a maximum: servers
    ///     treat it as a guide and may return more or fewer.
    ///   - page: ``Trips/earlier`` or ``Trips/later`` of an answer, to
    ///     continue that search with the same other arguments.
    /// - Throws: ``PublicTransportError``, such as `notFound` for a place
    ///   the network does not know, and `URLError` if the request got no
    ///   answer.
    public func trips(
        in network: TransitNetwork,
        from origin: some PlaceConvertible,
        to destination: some PlaceConvertible,
        departingAt: Date? = nil,
        arrivingBy: Date? = nil,
        limit: Int? = nil,
        page: String? = nil
    ) async throws -> Trips {
        let time: TripTime?
        switch (departingAt, arrivingBy) {
        case (let departure?, nil): time = .departAt(departure)
        case (nil, let arrival?): time = .arriveBy(arrival)
        case (nil, nil): time = nil
        case (.some, .some):
            throw PublicTransportError.invalidQuery(
                message: "a trip is bound to its departure or to its arrival, not to both")
        }
        let query = TripsQuery(
            origin: origin.place,
            destination: destination.place,
            time: time,
            limit: try count(limit),
            page: page
        )
        return try await trips(in: network, query: query)
    }

    /// Plans trips with a query of the core library. An answer that is an
    /// empty window of time, see `TransitNetwork.nextWindow(query:trips:)`,
    /// is followed with one more request.
    public func trips(in network: TransitNetwork, query: TripsQuery) async throws -> Trips {
        let response = try await send(network.tripsRequest(query: query))
        let trips = try network.parseTrips(query: query, response: response)
        guard let next = network.nextWindow(query: query, trips: trips) else { return trips }
        return try network.parseTrips(query: next, response: await send(network.tripsRequest(query: next)))
    }

    /// A number of results as the core library takes it.
    private func count(_ limit: Int?) throws -> UInt32? {
        guard let limit else { return nil }
        guard let count = UInt32(exactly: limit) else {
            throw PublicTransportError.invalidQuery(message: "the limit \(limit) is no number of results")
        }
        return count
    }

    private func send(_ request: HttpRequest) async throws -> HttpResponse {
        guard let url = URL(string: request.url) else {
            throw URLError(.badURL)
        }
        var urlRequest = URLRequest(url: url)
        switch request.method {
        case .get: urlRequest.httpMethod = "GET"
        case .post: urlRequest.httpMethod = "POST"
        }
        urlRequest.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        // After the user agent, so that one from the network takes its place.
        for header in request.headers {
            urlRequest.setValue(header.value, forHTTPHeaderField: header.name)
        }
        urlRequest.httpBody = request.body

        let (body, response): (Data, URLResponse)
        do {
            (body, response) = try await load(urlRequest)
        } catch let error as URLError where error.code == .cancelled {
            // What a cancelled task throws everywhere else in Swift.
            throw CancellationError()
        }
        guard let response = response as? HTTPURLResponse else {
            throw URLError(.badServerResponse)
        }
        let headers = response.allHeaderFields.compactMap { name, value in
            (name as? String).map { Header(name: $0, value: "\(value)") }
        }
        return HttpResponse(status: UInt16(clamping: response.statusCode), headers: headers, body: body)
    }
}

extension Client {
    /// Sends the request and reads the answer, up to the largest size.
    private func load(_ request: URLRequest) async throws -> (Data, URLResponse) {
        #if canImport(FoundationNetworking)
            // Foundation on Linux cannot read an answer in pieces, so a body
            // that is too large is only refused once it is there.
            let (body, response) = try await session.data(for: request, delegate: Unredirected())
            guard body.count <= Self.largestBody else {
                throw ClientError.responseTooLarge
            }
            return (body, response)
        #else
            let (bytes, response) = try await session.bytes(for: request, delegate: Unredirected())
            guard response.expectedContentLength <= Int64(Self.largestBody) else {
                throw ClientError.responseTooLarge
            }
            var body = Data()
            // Leaving the loop with an error cancels the download.
            for try await byte in bytes {
                guard body.count < Self.largestBody else {
                    throw ClientError.responseTooLarge
                }
                body.append(byte)
            }
            return (body, response)
        #endif
    }
}

/// Keeps `URLSession` from following a redirect: the core library reports
/// one as a moved endpoint.
private final class Unredirected: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest,
        completionHandler: @escaping @Sendable (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}
