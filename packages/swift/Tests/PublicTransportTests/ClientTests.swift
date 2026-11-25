import Foundation
import PublicTransport
import Testing

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

// Every request goes to an `HTTPStub`. Building requests and parsing answers
// is the Rust core's job and tested there, so the tests compare with what the
// core itself returns.

private let userAgent = "publictransport-tests/1.0"
/// 2026-11-16T13:24:00Z, the day the fixtures were recorded.
private let time = Date(timeIntervalSince1970: 1_794_835_440)
private let dresden = Coordinate(latitude: 51.04, longitude: 13.73)
/// A stop that another network returned.
private let foreignStop = Location(
    id: "8010085",
    globalIds: [GlobalId(scheme: .ifopt, value: "de:14612:28:3:2")],
    kind: .stop,
    name: "Dresden Hbf",
    locality: nil,
    coordinate: nil,
    timeZoneIdentifier: nil,
    network: "other"
)
private let hauptbahnhof = LocationQuery(text: "Hauptbahnhof")

private func fixture(_ path: String, file: String = #filePath) throws -> Data {
    var root = URL(fileURLWithPath: file)
    // The file, then PublicTransportTests, Tests, swift and packages.
    for _ in 1...5 { root.deleteLastPathComponent() }
    return try Data(contentsOf: root.appendingPathComponent("crates/core/tests/fixtures/\(path)"))
}

private func ok(_ body: Data) -> HttpResponse {
    HttpResponse(status: 200, headers: [], body: body)
}

@Sendable private func efa(_ host: String) -> TransitNetwork {
    TransitNetwork(
        id: "vvo", name: "VVO", endpoint: "https://\(host)/std3", protocol: .efa,
        timeZoneIdentifier: "Europe/Berlin"
    )
}

@Sendable private func motis(_ host: String) -> TransitNetwork {
    TransitNetwork(id: "transitous", name: "Transitous", endpoint: "https://\(host)/api", protocol: .motis)
}

/// TRIAS is the protocol whose requests have a body and a header of their
/// own, next to the one of the network.
@Sendable private func trias(_ host: String) -> TransitNetwork {
    TransitNetwork(
        id: "trias", name: "TRIAS", endpoint: "https://\(host)/trias", protocol: .trias,
        requestorRef: "OpenService", headers: [Header(name: "X-Api-Key", value: "secret")]
    )
}

/// One call of the client, next to the request the core builds for it.
struct Call: Sendable, CustomTestStringConvertible {
    let testDescription: String
    var network: @Sendable (String) -> TransitNetwork = efa
    let fixture: String
    let request: @Sendable (TransitNetwork) throws -> HttpRequest
    /// Makes the call and compares its result with what the core parses.
    let check: @Sendable (Client, TransitNetwork, HttpResponse) async throws -> Void

    static func locations(
        _ name: String, network: @escaping @Sendable (String) -> TransitNetwork = efa,
        fixture: String = "efa/vvo-stopfinder.json", query: LocationQuery,
        call: @escaping @Sendable (Client, TransitNetwork) async throws -> [Location]
    ) -> Call {
        Call(
            testDescription: name, network: network, fixture: fixture,
            request: { try $0.locationsRequest(query: query) },
            check: { try #expect(await call($0, $1) == $1.parseLocations(response: $2)) }
        )
    }

    static func nearby(
        _ name: String, query: NearbyQuery,
        call: @escaping @Sendable (Client, TransitNetwork) async throws -> [Location]
    ) -> Call {
        Call(
            testDescription: name, fixture: "efa/vvo-nearby.json",
            request: { try $0.nearbyRequest(query: query) },
            check: { try #expect(await call($0, $1) == $1.parseNearby(query: query, response: $2)) }
        )
    }

    static func departures(
        _ name: String, network: @escaping @Sendable (String) -> TransitNetwork = efa,
        fixture: String = "efa/vvo-departures.json", query: DeparturesQuery,
        call: @escaping @Sendable (Client, TransitNetwork) async throws -> [Departure]
    ) -> Call {
        Call(
            testDescription: name, network: network, fixture: fixture,
            request: { try $0.departuresRequest(query: query) },
            check: { try #expect(await call($0, $1) == $1.parseDepartures(response: $2)) }
        )
    }

    static func arrivals(
        _ name: String, query: DeparturesQuery,
        call: @escaping @Sendable (Client, TransitNetwork) async throws -> [Arrival]
    ) -> Call {
        Call(
            testDescription: name, fixture: "efa/vvo-arrivals.json",
            request: { try $0.arrivalsRequest(query: query) },
            check: { try #expect(await call($0, $1) == $1.parseArrivals(response: $2)) }
        )
    }

    /// The ID is the one of a departure in the answer that was recorded
    /// with the run.
    static let journey: Call = {
        let id: @Sendable (TransitNetwork) throws -> String = {
            let departures = try $0.parseDepartures(
                response: ok(PublicTransportTests.fixture("efa/vvo-journey-departure.json")))
            return try #require(departures.compactMap(\.journeyId).first)
        }
        return Call(
            testDescription: "journey of a departure", fixture: "efa/vvo-journey.json",
            request: { try $0.journeyRequest(id: id($0)) },
            check: { try #expect(await $0.journey(in: $1, id: id($1)) == $1.parseJourney(response: $2)) }
        )
    }()

    /// The query gets the reference to the trips after those of the fixture.
    static func trips(
        _ name: String, query: @escaping @Sendable (_ later: String?) -> TripsQuery,
        call: @escaping @Sendable (Client, TransitNetwork, _ later: String?) async throws -> Trips
    ) -> Call {
        let later: @Sendable (TransitNetwork) throws -> String? = {
            try $0.parseTrips(
                query: TripsQuery(origin: .id("de:14612:28"), destination: .id("de:14612:13")),
                response: ok(PublicTransportTests.fixture("efa/vvo-trip.json"))
            ).later
        }
        return Call(
            testDescription: name, fixture: "efa/vvo-trip.json",
            request: { try $0.tripsRequest(query: query(later($0))) },
            check: {
                let query = try query(later($1))
                try #expect(await call($0, $1, later($1)) == $1.parseTrips(query: query, response: $2))
            }
        )
    }

    static let all: [Call] = [
        .locations("locations by query", query: hauptbahnhof) {
            try await $0.locations(in: $1, query: hauptbahnhof)
        },
        .locations(
            "locations by query, with a body and headers",
            network: trias, fixture: "trias/vvo-locations.xml", query: hauptbahnhof
        ) {
            try await $0.locations(in: $1, query: hauptbahnhof)
        },
        .departures(
            "departures by query",
            network: motis, fixture: "motis/transitous-stoptimes.json",
            query: DeparturesQuery(stop: .id("de-DELFI_de:14612:28"))
        ) {
            try await $0.departures(in: $1, query: DeparturesQuery(stop: .id("de-DELFI_de:14612:28")))
        },
        .trips("trips by query") { _ in
            TripsQuery(origin: .id("de:14612:28"), destination: .id("de:14612:13"))
        } call: { client, network, _ in
            try await client.trips(
                in: network, query: TripsQuery(origin: .id("de:14612:28"), destination: .id("de:14612:13"))
            )
        },
        .locations(
            "locations by text, kinds and limit",
            query: LocationQuery(text: "Hauptbahnhof", limit: 3, kinds: [.stop])
        ) {
            try await $0.locations(in: $1, matching: "Hauptbahnhof", kinds: [.stop], limit: 3)
        },
        .nearby(
            "stops near a position, with radius and limit",
            query: NearbyQuery(
                position: Coordinate(latitude: 51.04, longitude: 13.7325), radiusMeters: 500, limit: 5)
        ) {
            try await $0.nearby(
                in: $1, at: Coordinate(latitude: 51.04, longitude: 13.7325), radiusMeters: 500, limit: 5)
        },
        .departures(
            "departures at an ID, with time and limit",
            query: DeparturesQuery(stop: .id("de:14612:28"), time: time, limit: 7)
        ) {
            try await $0.departures(in: $1, at: "de:14612:28", after: time, limit: 7)
        },
        .departures(
            "departures at a location of another network",
            query: DeparturesQuery(stop: .location(foreignStop))
        ) {
            try await $0.departures(in: $1, at: foreignStop)
        },
        .departures("departures at a place", query: DeparturesQuery(stop: .id("de:14612:28"))) {
            try await $0.departures(in: $1, at: Place.id("de:14612:28"))
        },
        .arrivals(
            "arrivals at an ID, with time and limit",
            query: DeparturesQuery(stop: .id("de:14612:28"), time: time, limit: 7)
        ) {
            try await $0.arrivals(in: $1, at: "de:14612:28", after: time, limit: 7)
        },
        .journey,
        .trips("trips from a location to an ID, departing at a time, with limit") { _ in
            TripsQuery(
                origin: .location(foreignStop), destination: .id("de:14612:13"),
                time: .departAt(time), limit: 3
            )
        } call: { client, network, _ in
            try await client.trips(
                in: network, from: foreignStop, to: "de:14612:13", departingAt: time, limit: 3
            )
        },
        .trips("trips from an ID to a coordinate, arriving by a time") { _ in
            TripsQuery(origin: .id("de:14612:28"), destination: .coordinate(dresden), time: .arriveBy(time))
        } call: { client, network, _ in
            try await client.trips(in: network, from: "de:14612:28", to: dresden, arrivingBy: time)
        },
        .trips("trips of the next page") { later in
            TripsQuery(origin: .id("de:14612:28"), destination: .id("de:14612:13"), page: later)
        } call: { client, network, later in
            try await client.trips(in: network, from: "de:14612:28", to: "de:14612:13", page: later)
        },
    ]
}

@Test(arguments: Call.all)
func sendsTheRequestOfTheCoreAndReturnsWhatTheCoreParses(call: Call) async throws {
    let body = try fixture(call.fixture)
    let stub = HTTPStub(.response(status: 200, body: [body]))
    let network = call.network(stub.host)
    let request = try call.request(network)

    try await call.check(Client(userAgent: userAgent, session: stub.session), network, ok(body))

    let sent = stub.sent
    try #require(sent.count == 1)
    #expect(sent[0].request.httpMethod == (request.method == .post ? "POST" : "GET"))
    #expect(sent[0].request.url?.absoluteString == request.url)
    #expect(sent[0].body == request.body)
    for header in request.headers {
        #expect(sent[0].request.value(forHTTPHeaderField: header.name) == header.value)
    }
    #expect(sent[0].request.value(forHTTPHeaderField: "User-Agent") == userAgent)
}

@Test func userAgentOfTheNetworkWins() async throws {
    let stub = try HTTPStub(.response(status: 200, body: [fixture("efa/vvo-stopfinder.json")]))
    var network = efa(stub.host)
    network.headers = [Header(name: "User-Agent", value: "network/1.0")]

    _ = try await Client(userAgent: userAgent, session: stub.session)
        .locations(in: network, query: hauptbahnhof)

    #expect(stub.sent.map { $0.request.value(forHTTPHeaderField: "User-Agent") } == ["network/1.0"])
}

/// An answer the core refuses. The stop of the last one is unknown.
struct Refused: Sendable, CustomTestStringConvertible {
    let testDescription: String
    let status: Int
    var headers: [String: String] = [:]
    var fixture: String?

    static let all = [
        Refused(testDescription: "moved", status: 302, headers: ["Location": "https://new.example/std3"]),
        Refused(testDescription: "rateLimited", status: 429),
        Refused(testDescription: "notFound", status: 200, fixture: "efa/vvo-departures-unknown-stop.json"),
    ]
}

@Test(arguments: Refused.all)
func errorOfTheCoreComesOutUnchanged(answer: Refused) async throws {
    let body = try answer.fixture.map { try fixture($0) } ?? Data()
    let stub = HTTPStub(.response(status: answer.status, headers: answer.headers, body: [body]))
    let network = efa(stub.host)
    // Status, headers and body all have to reach the core for the same error.
    let expected = #expect(throws: PublicTransportError.self) {
        try network.parseDepartures(
            response: HttpResponse(
                status: UInt16(answer.status),
                headers: answer.headers.map { Header(name: $0.key, value: $0.value) },
                body: body
            ))
    }
    #expect(expected.map { "\($0)" }?.hasPrefix(answer.testDescription) == true)

    let thrown = await #expect(throws: PublicTransportError.self) {
        try await Client(userAgent: userAgent, session: stub.session)
            .departures(in: network, at: "de:14612:28")
    }

    #expect(thrown == expected)
    // A redirect is not followed.
    #expect(stub.sent.count == 1)
}

@Test func failedExchangeIsTheErrorOfURLSession() async {
    let stub = HTTPStub(.failure(.cannotConnectToHost))

    let thrown = await #expect(throws: URLError.self) {
        try await Client(userAgent: userAgent, session: stub.session)
            .locations(in: efa(stub.host), query: hauptbahnhof)
    }

    #expect(thrown?.code == .cannotConnectToHost)
}

@Test func cancelledExchangeIsACancellationError() async {
    // What `URLSession` reports for a task that was cancelled.
    let stub = HTTPStub(.failure(.cancelled))

    await #expect(throws: CancellationError.self) {
        try await Client(userAgent: userAgent, session: stub.session)
            .locations(in: efa(stub.host), query: hauptbahnhof)
    }
}

@Test func tooLargeAnswerIsRefused() async {
    // 17 MB without a `Content-Length`, as a hostile server would send it.
    let megabyte = Data(repeating: UInt8(ascii: " "), count: 1024 * 1024)
    let stub = HTTPStub(.response(status: 200, body: Array(repeating: megabyte, count: 17)))

    await #expect(throws: ClientError.responseTooLarge) {
        try await Client(userAgent: userAgent, session: stub.session)
            .locations(in: efa(stub.host), query: hauptbahnhof)
    }
}

/// A plain call with values that no query can hold.
struct Rejected: Sendable, CustomTestStringConvertible {
    let testDescription: String
    let call: @Sendable (Client, TransitNetwork) async throws -> Void

    static let all = [
        Rejected(testDescription: "trips departing at and arriving by a time") {
            _ = try await $0.trips(
                in: $1, from: "de:14612:28", to: "de:14612:13", departingAt: time, arrivingBy: time
            )
        },
        Rejected(testDescription: "trips with a limit above UInt32.max") {
            _ = try await $0.trips(
                in: $1, from: "de:14612:28", to: "de:14612:13", limit: Int(UInt32.max) + 1
            )
        },
        Rejected(testDescription: "departures with a negative limit") {
            _ = try await $0.departures(in: $1, at: "de:14612:28", limit: -1)
        },
        Rejected(testDescription: "locations with a negative limit") {
            _ = try await $0.locations(in: $1, matching: "Hauptbahnhof", limit: -1)
        },
    ]
}

@Test(arguments: Rejected.all)
func invalidPlainCallIsRefusedBeforeARequest(rejected: Rejected) async {
    let stub = HTTPStub(.response(status: 200, body: []))

    let thrown = await #expect(throws: PublicTransportError.self) {
        try await rejected.call(Client(userAgent: userAgent, session: stub.session), efa(stub.host))
    }

    guard case .invalidQuery = thrown else {
        Issue.record("expected invalidQuery, got \(String(describing: thrown))")
        return
    }
    #expect(stub.sent.isEmpty)
}
