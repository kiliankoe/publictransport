import Foundation
import PublicTransport
import Testing

// The model is generated from the Rust core, which tests its values. These
// tests cover what the Swift side adds: names, descriptions, conveniences and
// storing values as JSON.

private let dresden = Coordinate(latitude: 51.04, longitude: 13.73)
private let secret = Header(name: "X-Api-Key", value: "hunter2")
/// A query that keeps every trip of an answer.
private let anyTrips = TripsQuery(origin: .id("a"), destination: .id("b"))

private func file(_ path: String, file: String = #filePath) throws -> Data {
    var root = URL(fileURLWithPath: file)
    // The file, then PublicTransportTests, Tests, swift and packages.
    for _ in 1...5 { root.deleteLastPathComponent() }
    return try Data(contentsOf: root.appendingPathComponent("crates/core/\(path)"))
}

private func answer(_ fixture: String) throws -> HttpResponse {
    try HttpResponse(status: 200, headers: [], body: file("tests/fixtures/\(fixture)"))
}

private func stop(_ ifopt: String, timezone: String? = nil) -> Location {
    Location(
        id: ifopt, globalIds: [GlobalId(scheme: .ifopt, value: ifopt)], kind: .stop, name: "Stop",
        locality: nil, coordinate: nil, timeZoneIdentifier: timezone, network: nil
    )
}

@Test func derivedValuesOfDeparturesAndTripsAreProperties() throws {
    let vvo = TransitNetwork.bundled(.vvoEfa)
    let departures = try vvo.parseDepartures(response: answer("efa/vvo-departures.json"))
    let trips = try vvo.parseTrips(query: anyTrips, response: answer("efa/vvo-trip.json")).trips
    try #require(!departures.isEmpty && !trips.isEmpty)

    #expect(departures.first?.delaySeconds == 60)
    #expect(trips.first?.legs.first?.departure.platform == "14")
    #expect(trips.first?.durationSeconds == 840)
    for departure in departures {
        #expect(departure.expectedTime == departure.realtimeTime ?? departure.scheduledTime)
    }
    for trip in trips {
        let rides = trip.legs.count { if case .transit = $0.kind { true } else { false } }
        #expect(trip.departure == trip.legs.first?.departure)
        #expect(trip.arrival == trip.legs.last?.arrival)
        #expect(trip.transfers == UInt32(max(rides - 1, 0)))
        for end in trip.legs.flatMap({ [$0.departure, $0.arrival] }) {
            #expect(end.expectedTime == end.realtimeTime ?? end.scheduledTime)
        }
    }
}

@Test func remarksCrossTheBoundary() throws {
    let departures = try TransitNetwork.bundled(.vrrEfa).parseDepartures(
        response: answer("efa/vrr-departures.json"))

    let remark = try #require(departures.flatMap(\.remarks).first { $0.id == "314433" })
    #expect(remark.kind == .disruption)
    #expect(remark.title?.hasPrefix("Fahrleitungsarbeiten in Heerdt") == true)
    #expect(remark.validFrom == Date(timeIntervalSince1970: 1_794_812_400))
}

@Test func locationKnowsItsStationAndItsTimeZone() {
    let station = stop("de:14612:28", timezone: "Europe/Berlin")
    let platform = stop("de:14612:28:3:2", timezone: "Nowhere/Nothing")
    let elsewhere = stop("de:14612:13")

    #expect(!station.stationIds.isEmpty)
    #expect(platform.stationIds == station.stationIds)
    #expect(platform.isSameStation(with: station))
    #expect(!platform.isSameStation(with: elsewhere))

    #expect(station.timeZone == TimeZone(identifier: "Europe/Berlin"))
    #expect(station.timeZone != nil)
    #expect(platform.timeZone == nil)
    #expect(elsewhere.timeZone == nil)
}

@Test func registryFindsNetworksByIDAndByPosition() {
    let registry = Registry.bundled()
    let vvo = TransitNetwork.bundled(.vvoEfa)

    #expect(BundledNetwork.vvoEfa.id == "vvo-efa")
    #expect(vvo == TransitNetwork.bundled(.vvoEfa))
    #expect(registry.network(id: "vvo-efa") == vvo)
    #expect(registry.network(id: "no-such-network") == nil)
    // A case is sent to Rust as its position in the list, so the order matters.
    // The registry has one network of each operator, in that order.
    let listed = BundledNetwork.allCases.map(\.id).filter { registry.network(id: $0) != nil }
    #expect(listed == registry.networks().map(\.id))
    #expect(registry.network(id: "vvo-trias") == nil)
    #expect(TransitNetwork.bundled(.vvoTrias).id == "vvo-trias")

    #expect(vvo.coverage.tier(at: dresden) == .realtime)
    #expect(vvo.coverage.tier(at: Coordinate(latitude: 0, longitude: 0)) == nil)
    #expect(registry.covering(at: dresden).contains(CoveringNetwork(network: vvo, tier: .realtime)))
}

@Test func distanceBetweenPositionsCrossesTheBoundary() {
    let dresden = Coordinate(latitude: 51, longitude: 13.7)

    #expect(abs(dresden.distanceMeters(to: Coordinate(latitude: 52, longitude: 13.7)) - 111_200) < 100)
}

@Test func enumsWithoutValuesListTheirCases() {
    #expect(LocationKind.allCases == [.stop, .address, .poi, .other])
    #expect(CoverageTier.allCases == [.realtime, .regular, .any])
    #expect(IdScheme.allCases.contains(.ifopt))
    #expect(Mode.allCases.contains(.regionalTrain))
    #expect(NetworkProtocol.allCases.contains(.efa))
    #expect(!NetworkOperation.allCases.isEmpty)
}

@Test func errorCasesAreLowerCamelCaseAndDescribeThemselves() {
    // Naming every case is the test: it does not compile with another spelling.
    let all: Set<PublicTransportError> = [
        .invalidNetwork(message: ""), .invalidQuery(message: ""), .unsupported(message: ""),
        .notFound(message: ""), .moved(location: ""), .blocked, .rateLimited(retryAfterSeconds: nil),
        .unauthorized, .unavailable, .server(status: 500, message: ""), .parse(message: ""),
    ]
    #expect(all.count == 11)

    let wait = [Header(name: "Retry-After", value: "120")]
    let thrown = #expect(throws: PublicTransportError.self) {
        try TransitNetwork.bundled(.vvoEfa)
            .parseDepartures(response: HttpResponse(status: 429, headers: wait, body: Data()))
    }
    #expect(thrown == .rateLimited(retryAfterSeconds: 120))
    #expect(thrown?.localizedDescription == "the server received too many requests")
}

@Test func descriptionsAreSwiftsOwnAndHideSecrets() throws {
    #expect(String(describing: LocationKind.stop) == "stop")

    let locations = try TransitNetwork.bundled(.vvoEfa).parseLocations(
        response: answer("efa/vvo-stopfinder.json"))
    let location = try #require(locations.first)
    #expect(String(describing: location).contains("globalIds"))
    #expect(!String(describing: location).contains("global_ids"))

    var network = TransitNetwork.bundled(.vvoEfa)
    network.headers = [secret]
    let printed = [
        String(describing: secret), String(reflecting: secret),
        String(describing: network), String(reflecting: network),
    ]
    for text in printed {
        #expect(!text.contains("hunter2"))
    }
    // The name is no secret, and shows that the header is printed at all.
    #expect(String(describing: secret).contains("X-Api-Key"))
}

@Test func networkIsReadFromAnEntryOfTransportAPIs() throws {
    let json = try String(decoding: file("data/networks/vvo-efa.json"), as: UTF8.self)

    #expect(try TransitNetwork(id: "mine", json: json).id == "mine")
    // The entry is the one of the bundled network.
    #expect(try TransitNetwork(id: "vvo-efa", json: json) == TransitNetwork.bundled(.vvoEfa))

    let thrown = #expect(throws: PublicTransportError.self) {
        try TransitNetwork(id: "mine", json: "no entry")
    }
    guard case .invalidNetwork = thrown else {
        Issue.record("expected invalidNetwork, got \(String(describing: thrown))")
        return
    }
}

/// The value after an app stored it and read it back.
private func stored<Value: Codable>(_ value: Value) throws -> Value {
    try JSONDecoder().decode(Value.self, from: JSONEncoder().encode(value))
}

/// The text each value is stored as. In an array, because not every platform encodes a lone string.
private func storedText(_ values: [some Encodable]) throws -> [String] {
    try JSONDecoder().decode([String].self, from: JSONEncoder().encode(values))
}

@Test func enumsWithoutValuesAreStoredAsTheNamesOfTheirCases() throws {
    #expect(try storedText([Mode.bus, Mode.regionalTrain]) == ["bus", "regionalTrain"])
    #expect(try stored(Mode.allCases) == Mode.allCases)
    #expect(throws: DecodingError.self) {
        _ = try JSONDecoder().decode([Mode].self, from: Data(#"["noSuchCase"]"#.utf8))
    }
}

@Test func bundledNetworkIsStoredAsItsID() throws {
    #expect(try storedText([BundledNetwork.vvoEfa]) == ["vvo-efa"])
    #expect(try storedText(BundledNetwork.allCases) == BundledNetwork.allCases.map(\.id))
    #expect(try stored(BundledNetwork.allCases) == BundledNetwork.allCases)
}

@Test func storedAnswersAreReadBackEqual() throws {
    let vvo = TransitNetwork.bundled(.vvoEfa)
    let locations = try vvo.parseLocations(response: answer("efa/vvo-stopfinder.json"))
    let departures = try vvo.parseDepartures(response: answer("efa/vvo-departures.json"))
    let trips = try vvo.parseTrips(query: anyTrips, response: answer("efa/vvo-trip.json"))
    try #require(!locations.isEmpty && !departures.isEmpty && !trips.trips.isEmpty)

    #expect(try stored(locations) == locations)
    #expect(try stored(departures) == departures)
    #expect(try stored([trips]) == [trips])
}

@Test func storedQueriesAreReadBackEqual() throws {
    // Every kind of place and of time that a query can hold.
    let time = Date(timeIntervalSince1970: 1_794_835_440)
    let locations = LocationQuery(text: "Hauptbahnhof", limit: 3, kinds: [.stop, .poi])
    let departures = DeparturesQuery(stop: .location(stop("de:14612:28")), time: time, limit: 7)
    let trips = [
        TripsQuery(
            origin: .id("de:14612:28"), destination: .coordinate(dresden),
            time: .departAt(time), limit: 3, page: "later"
        ),
        TripsQuery(origin: .coordinate(dresden), destination: .id("de:14612:13"), time: .arriveBy(time)),
    ]

    #expect(try stored([locations]) == [locations])
    #expect(try stored([departures]) == [departures])
    #expect(try stored(trips) == trips)
}
