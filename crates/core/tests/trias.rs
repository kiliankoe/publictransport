//! TRIAS 1.2 and OJP 1.0: the XML requests and the parsing of responses.

use proptest::prelude::*;
use publictransport::{
    Coordinate, Departure, DeparturesQuery, Error, GlobalId, HttpRequest, HttpResponse, IdScheme,
    Leg, LegEnd, LegKind, Location, LocationKind, LocationQuery, Method, Mode, Network, Place,
    Protocol, Stopover, Timestamp, TripsQuery,
};

const TRIAS_LOCATIONS: &[u8] = include_bytes!("fixtures/trias/vvo-locations.xml");
const TRIAS_NO_MATCH: &[u8] = include_bytes!("fixtures/trias/vvo-locations-no-match.xml");
const TRIAS_VVO: &[u8] = include_bytes!("fixtures/trias/vvo-departures.xml");
const TRIAS_UNKNOWN_STOP: &[u8] = include_bytes!("fixtures/trias/vvo-departures-unknown-stop.xml");
const TRIAS_VRR: &[u8] = include_bytes!("fixtures/trias/vrr-departures.xml");
const TRIAS_TRIP: &[u8] = include_bytes!("fixtures/trias/vvo-trip.xml");
const OJP_LOCATIONS: &[u8] = include_bytes!("fixtures/ojp/vrr-locations.xml");
const OJP_VRR: &[u8] = include_bytes!("fixtures/ojp/vrr-departures.xml");
const OJP_NO_EVENT: &[u8] = include_bytes!("fixtures/ojp/vvo-departures.xml");
const OJP_TRIP: &[u8] = include_bytes!("fixtures/ojp/vrr-trip.xml");

const PROTOCOLS: [Protocol; 2] = [Protocol::Trias, Protocol::Ojp1];

/// The three operations, by the name their response element is built from.
const KINDS: [&str; 3] = ["LocationInformation", "StopEvent", "Trip"];

const ENDPOINT: &str = "https://example.org/std3/trias";

/// Every character an XML 1.0 document can carry.
const XML_TEXT: &str = r"[\t\n\r\x20-\u{D7FF}\u{E000}-\u{FFFD}\u{10000}-\u{10FFFF}]*";

fn network(protocol: Protocol) -> Network {
    Network::new("test", "Test", ENDPOINT, protocol).with_requestor_ref("OpenService")
}

fn response(body: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

/// A time of day on 2026-11-16 in UTC, the day the fixtures were recorded.
fn utc(hour: i64, minute: i64, second: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_794_787_200 + hour * 3600 + minute * 60 + second,
    }
}

fn text(fixture: &[u8]) -> String {
    String::from_utf8_lossy(fixture).into_owned()
}

/// The namespace prefix the fixtures of a protocol use for most elements, and
/// the one for `StopPointRef` and the submodes, which OJP takes from SIRI.
fn prefixes(protocol: Protocol) -> (&'static str, &'static str) {
    if protocol == Protocol::Trias {
        ("trias", "trias")
    } else {
        ("ojp", "siri")
    }
}

/// A recorded response with results for one of the [`KINDS`].
fn fixture(protocol: Protocol, kind: &str) -> &'static [u8] {
    match (protocol == Protocol::Trias, kind) {
        (true, "LocationInformation") => TRIAS_LOCATIONS,
        (true, "StopEvent") => TRIAS_VRR,
        (true, _) => TRIAS_TRIP,
        (false, "LocationInformation") => OJP_LOCATIONS,
        (false, "StopEvent") => OJP_VRR,
        (false, _) => OJP_TRIP,
    }
}

/// Parses `body` as the response for one of the [`KINDS`] and counts the
/// results.
fn count(protocol: Protocol, kind: &str, body: &[u8]) -> Result<usize, Error> {
    let network = network(protocol);
    let response = response(body);
    match kind {
        "LocationInformation" => network.parse_locations(&response).map(|list| list.len()),
        "StopEvent" => network.parse_departures(&response).map(|list| list.len()),
        _ => network
            .parse_trips(&response)
            .map(|found| found.trips.len()),
    }
}

/// An element of a request body. Only an element without child elements has
/// text.
#[derive(Debug)]
struct Element {
    namespace: String,
    name: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<Self>,
}

fn element(node: roxmltree::Node) -> Element {
    let children: Vec<_> = node
        .children()
        .filter(roxmltree::Node::is_element)
        .map(element)
        .collect();
    let text = if children.is_empty() {
        node.children().filter_map(|child| child.text()).collect()
    } else {
        String::new()
    };
    Element {
        namespace: node.tag_name().namespace().unwrap_or_default().to_owned(),
        name: node.tag_name().name().to_owned(),
        attributes: node
            .attributes()
            .map(|attribute| (attribute.name().to_owned(), attribute.value().to_owned()))
            .collect(),
        text,
        children,
    }
}

fn body(request: &HttpRequest) -> &str {
    std::str::from_utf8(request.body.as_deref().unwrap_or_default()).unwrap_or_default()
}

/// The first place where `actual` is not the element `expected` describes.
/// The expected text `*` stands for any text, and degrees are compared as
/// numbers.
fn mismatch(actual: &Element, expected: &Element) -> Option<String> {
    let degrees = |element: &Element| element.text.parse::<f64>().ok();
    let same_text = expected.text == "*"
        || actual.text == expected.text
        || matches!(expected.name.as_str(), "Longitude" | "Latitude")
            && degrees(actual)
                .zip(degrees(expected))
                .is_some_and(|(actual, expected)| (actual - expected).abs() < 1e-6);
    let same = same_text
        && (&actual.namespace, &actual.name, &actual.attributes)
            == (&expected.namespace, &expected.name, &expected.attributes)
        && actual.children.len() == expected.children.len();
    if !same {
        return Some(format!("expected {expected:#?}, found {actual:#?}"));
    }
    actual
        .children
        .iter()
        .zip(&expected.children)
        .find_map(|(actual, expected)| mismatch(actual, expected))
}

/// How the body of `request` differs from the XML document `expected`.
fn body_mismatch(request: &HttpRequest, expected: &str) -> Option<String> {
    let actual = match roxmltree::Document::parse(body(request)) {
        Ok(document) => element(document.root_element()),
        Err(error) => return Some(format!("{error}: {}", body(request))),
    };
    match roxmltree::Document::parse(expected) {
        Ok(document) => mismatch(&actual, &element(document.root_element())),
        Err(error) => Some(format!("{error}: {expected}")),
    }
}

/// The text of every element called `name` in the body of `request`.
fn texts(request: &HttpRequest, name: &str) -> Result<Vec<String>, roxmltree::Error> {
    let document = roxmltree::Document::parse(body(request))?;
    Ok(document
        .descendants()
        .filter(|node| node.tag_name().name() == name)
        .map(|node| element(node).text)
        .collect())
}

/// The request document of a protocol around the request called `name`. The
/// content is written without prefixes: it is in the TRIAS namespace or the
/// OJP one.
fn envelope(protocol: Protocol, name: &str, content: &str) -> String {
    if protocol == Protocol::Trias {
        format!(
            r#"<Trias xmlns="http://www.vdv.de/trias" xmlns:siri="http://www.siri.org.uk/siri" version="1.2">
                <ServiceRequest>
                    <siri:RequestorRef>OpenService</siri:RequestorRef>
                    <RequestPayload><{name}>{content}</{name}></RequestPayload>
                </ServiceRequest>
            </Trias>"#
        )
    } else {
        format!(
            r#"<OJP xmlns="http://www.siri.org.uk/siri" version="1.0">
                <OJPRequest><ServiceRequest>
                    <RequestorRef>OpenService</RequestorRef>
                    <OJP{name} xmlns="http://www.vdv.de/ojp">{content}</OJP{name}>
                </ServiceRequest></OJPRequest>
            </OJP>"#
        )
    }
}

/// How a request names the stop `de:14612:28`.
fn stop_ref(protocol: Protocol) -> &'static str {
    if protocol == Protocol::Trias {
        "<LocationRef>
            <StopPointRef>de:14612:28</StopPointRef>
            <LocationName><Text>de:14612:28</Text></LocationName>
        </LocationRef>"
    } else {
        "<PlaceRef>
            <StopPlaceRef>de:14612:28</StopPlaceRef>
            <LocationName><Text>de:14612:28</Text></LocationName>
        </PlaceRef>"
    }
}

/// How a request names the position 51.0633° north, 13.7467° east.
fn position_ref(protocol: Protocol) -> &'static str {
    if protocol == Protocol::Trias {
        "<LocationRef>
            <GeoPosition><Longitude>13.7467</Longitude><Latitude>51.0633</Latitude></GeoPosition>
            <LocationName><Text>*</Text></LocationName>
        </LocationRef>"
    } else {
        r#"<PlaceRef>
            <GeoPosition>
                <Longitude xmlns="http://www.siri.org.uk/siri">13.7467</Longitude>
                <Latitude xmlns="http://www.siri.org.uk/siri">51.0633</Latitude>
            </GeoPosition>
            <LocationName><Text>*</Text></LocationName>
        </PlaceRef>"#
    }
}

fn ifopt(value: &str) -> GlobalId {
    GlobalId {
        scheme: IdScheme::Ifopt,
        value: value.to_owned(),
    }
}

type LocationFields<'a> = (
    &'a str,
    Vec<GlobalId>,
    LocationKind,
    &'a str,
    Option<&'a str>,
    Option<(f64, f64)>,
);

/// Id, global ids, kind, name, locality and coordinate of a location.
fn fields(location: &Location) -> LocationFields<'_> {
    (
        &location.id,
        location.global_ids.clone(),
        location.kind,
        &location.name,
        location.locality.as_deref(),
        location.coordinate.map(|c| (c.latitude, c.longitude)),
    )
}

/// A time of day on 2026-11-16 as `hh:mm:ss`.
fn clock(time: Timestamp) -> String {
    let seconds = time.unix_seconds - utc(0, 0, 0).unix_seconds;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// Scheduled and predicted time, stop id, scheduled and current platform and
/// stop name of a call, with `-` for what it lacks.
fn call(
    stop: &Location,
    scheduled: Timestamp,
    realtime: Option<Timestamp>,
    platforms: [&Option<String>; 2],
) -> String {
    let [planned, current] = platforms.map(|platform| platform.as_deref().unwrap_or("-"));
    let realtime = realtime.map_or_else(|| "-".to_owned(), clock);
    let (scheduled, id, name) = (clock(scheduled), &stop.id, &stop.name);
    format!("{scheduled} {realtime} {id} {planned}/{current} {name}")
}

/// One end of a leg as its [`call`].
fn end(end: &LegEnd) -> String {
    let platforms = [&end.scheduled_platform, &end.realtime_platform];
    call(&end.stop, end.scheduled_time, end.realtime_time, platforms)
}

/// A departure as its [`call`], mode and direction.
fn summary(departure: &Departure) -> String {
    let platforms = [&departure.scheduled_platform, &departure.realtime_platform];
    format!(
        "{} {:?} {}",
        call(
            &departure.stop,
            departure.scheduled_time,
            departure.realtime_time,
            platforms
        ),
        departure.line.mode,
        departure.direction.as_deref().unwrap_or("-")
    )
}

/// Scheduled and predicted arrival, then scheduled and predicted departure,
/// at a call between the two ends of a leg.
fn times(stopover: &Stopover) -> [Option<String>; 4] {
    [
        stopover.scheduled_arrival,
        stopover.realtime_arrival,
        stopover.scheduled_departure,
        stopover.realtime_departure,
    ]
    .map(|time| time.map(clock))
}

/// Line name, mode, product, operator and direction of a ride.
type Ride<'a> = (
    &'a str,
    Mode,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
);

fn ride(leg: &Leg) -> Option<Ride<'_>> {
    match &leg.kind {
        LegKind::Transit { line, direction } => Some((
            &line.name,
            line.mode,
            line.product.as_deref(),
            line.operator.as_deref(),
            direction.as_deref(),
        )),
        _ => None,
    }
}

/// The line name of a ride, or how else the leg is traveled.
fn label(leg: &Leg) -> &str {
    match &leg.kind {
        LegKind::Transit { line, .. } => &line.name,
        LegKind::Walk => "walk",
        _ => "other",
    }
}

/// The response with an error and no results that each server sent for a
/// departures request, turned into the response for one of the [`KINDS`]
/// with the error `message`.
fn error_body(protocol: Protocol, kind: &str, message: &str) -> String {
    if protocol == Protocol::Trias {
        text(TRIAS_UNKNOWN_STOP)
            .replace("StopEventResponse", &format!("{kind}Response"))
            .replace("STOPEVENT_INTERNAL_ERROR", message)
    } else {
        text(OJP_NO_EVENT)
            .replace("OJPStopEventDelivery", &format!("OJP{kind}Delivery"))
            .replace("STOPEVENT_NOEVENTFOUND", message)
    }
}

/// The recorded trips with one more leg at the end of the last trip. The leg
/// is the element `leg` and has no vehicle; `mode` is the path of the element
/// that holds `value`.
fn trips_with_leg(protocol: Protocol, leg: &str, mode: &str, value: &str) -> String {
    let (p, s) = prefixes(protocol);
    let mode_element = mode.rsplit('/').fold(value.to_owned(), |inner, name| {
        format!("<{p}:{name}>{inner}</{p}:{name}>")
    });
    // A mode of its own comes first, one inside `Service` after the ends.
    let (before, after) = if mode.contains('/') {
        ("", mode_element.as_str())
    } else {
        (mode_element.as_str(), "")
    };
    let leg = format!(
        "<{p}:TripLeg><{p}:LegId>9</{p}:LegId><{p}:{leg}>{before}\
         <{p}:LegStart><{s}:StopPointRef>de:05113:9289:91</{s}:StopPointRef>\
         <{p}:LocationName><{p}:Text>Essen Hauptbahnhof</{p}:Text></{p}:LocationName></{p}:LegStart>\
         <{p}:LegEnd><{s}:StopPointRef>de:05113:9143</{s}:StopPointRef>\
         <{p}:LocationName><{p}:Text>Essen Rathaus</{p}:Text></{p}:LocationName></{p}:LegEnd>{after}\
         <{p}:TimeWindowStart>2026-11-16T23:54:00Z</{p}:TimeWindowStart>\
         <{p}:TimeWindowEnd>2026-11-16T23:59:30Z</{p}:TimeWindowEnd>\
         <{p}:Duration>PT5M30S</{p}:Duration></{p}:{leg}></{p}:TripLeg>"
    );
    let end = if protocol == Protocol::Trias {
        "</trias:Trip></trias:TripResult></trias:TripResponse>"
    } else {
        "</ojp:Trip></ojp:TripResult></ojp:OJPTripDelivery>"
    };
    text(fixture(protocol, "Trip")).replace(end, &format!("{leg}{end}"))
}

proptest! {
    #[test]
    fn every_request_posts_the_text_intact_as_xml_to_the_endpoint_as_given(
        search in XML_TEXT,
        origin in XML_TEXT,
        destination in XML_TEXT,
        requestor in XML_TEXT.prop_filter("a requestor reference is required", |text| !text.is_empty()),
        trailing_slash: bool,
    ) {
        let endpoint = if trailing_slash { "https://example.org/trias/" } else { ENDPOINT };
        let trips_query =
            TripsQuery::new(Place::Id(origin.clone()), Place::Id(destination.clone()));
        for protocol in PROTOCOLS {
            let network = Network::new("test", "Test", endpoint, protocol)
                .with_requestor_ref(requestor.clone());
            let stop = if protocol == Protocol::Trias { "StopPointRef" } else { "StopPlaceRef" };

            let locations = network.locations_request(&LocationQuery::new(search.clone())).unwrap();
            prop_assert_eq!(texts(&locations, "LocationName").unwrap(), [search.clone()]);
            let departures =
                network.departures_request(&DeparturesQuery::new(origin.clone())).unwrap();
            prop_assert_eq!(texts(&departures, stop).unwrap(), [origin.clone()]);
            let trips = network.trips_request(&trips_query).unwrap();
            prop_assert_eq!(
                texts(&trips, stop).unwrap(),
                [origin.clone(), destination.clone()]
            );
            if protocol == Protocol::Ojp1 {
                // The id stands in for the name the schema demands.
                prop_assert_eq!(texts(&departures, "Text").unwrap(), [origin.clone()]);
                prop_assert_eq!(
                    texts(&trips, "Text").unwrap(),
                    [origin.clone(), destination.clone()]
                );
            }

            for request in [locations, departures, trips] {
                prop_assert_eq!(texts(&request, "RequestorRef").unwrap(), [requestor.clone()]);
                prop_assert_eq!(request.method, Method::Post);
                prop_assert_eq!(&request.url, endpoint);
                let content_types: Vec<_> = request
                    .headers
                    .iter()
                    .filter(|header| header.name.eq_ignore_ascii_case("content-type"))
                    .map(|header| header.value.as_str())
                    .collect();
                prop_assert_eq!(content_types, ["application/xml"]);
            }
        }
    }

    #[test]
    fn locations_request_has_one_type_for_each_wanted_kind(
        kinds in prop::collection::vec(
            prop::sample::select(vec![
                LocationKind::Stop,
                LocationKind::Address,
                LocationKind::Poi,
                LocationKind::Other,
            ]),
            0..8,
        ),
    ) {
        for (protocol, other) in [(Protocol::Trias, "locality"), (Protocol::Ojp1, "topographicPlace")] {
            let expected: std::collections::BTreeSet<_> = kinds
                .iter()
                .map(|kind| match kind {
                    LocationKind::Stop => "stop",
                    LocationKind::Address => "address",
                    LocationKind::Poi => "poi",
                    _ => other,
                })
                .collect();
            let query = LocationQuery::new("Hauptbahnhof").with_kinds(kinds.clone());
            let request = network(protocol).locations_request(&query).unwrap();
            let mut types = texts(&request, "Type").unwrap();
            types.sort_unstable();
            prop_assert_eq!(types, expected.into_iter().collect::<Vec<_>>(), "{:?}", protocol);
        }
    }

    #[test]
    fn parsing_never_panics(
        body in prop_oneof![
            any::<Vec<u8>>(),
            (0..PROTOCOLS.len(), 0..KINDS.len(), any::<prop::sample::Index>()).prop_map(
                |(protocol, kind, end)| {
                    let fixture = fixture(PROTOCOLS[protocol], KINDS[kind]);
                    fixture[..end.index(fixture.len())].to_vec()
                }
            ),
        ],
    ) {
        for protocol in PROTOCOLS {
            for kind in KINDS {
                let _ = count(protocol, kind, &body);
            }
        }
    }
}

#[test]
fn request_without_a_requestor_reference_is_an_invalid_network() {
    let place = || Place::Id("de:14612:28".to_owned());
    for protocol in PROTOCOLS {
        let network = Network::new("test", "Test", ENDPOINT, protocol);
        for result in [
            network.locations_request(&LocationQuery::new("Hauptbahnhof")),
            network.departures_request(&DeparturesQuery::new("de:14612:28")),
            network.trips_request(&TripsQuery::new(place(), place())),
        ] {
            assert!(
                matches!(result, Err(Error::InvalidNetwork { .. })),
                "{protocol:?}: {result:?}"
            );
        }
    }
}

#[test]
fn time_outside_the_calendar_is_an_invalid_query() {
    let place = || Place::Id("de:14612:28".to_owned());
    for protocol in PROTOCOLS {
        let network = network(protocol);
        for unix_seconds in [i64::MAX, i64::MIN] {
            let time = Timestamp { unix_seconds };
            for result in [
                network.departures_request(&DeparturesQuery::new("de:14612:28").with_time(time)),
                network.trips_request(&TripsQuery::new(place(), place()).with_departure(time)),
                network.trips_request(&TripsQuery::new(place(), place()).with_arrival(time)),
            ] {
                assert!(
                    matches!(result, Err(Error::InvalidQuery { .. })),
                    "{protocol:?}: {result:?}"
                );
            }
        }
    }
}

#[test]
fn locations_request_carries_the_text_and_only_the_restrictions_the_query_has() {
    let query = LocationQuery::new("Hauptbahnhof");
    for protocol in PROTOCOLS {
        for (query, restrictions) in [
            (query.clone(), ""),
            (
                query.clone().with_limit(5),
                "<Restrictions><NumberOfResults>5</NumberOfResults></Restrictions>",
            ),
            (
                query.clone().with_kinds([LocationKind::Stop]),
                "<Restrictions><Type>stop</Type></Restrictions>",
            ),
            (
                query.clone().with_limit(5).with_kinds([LocationKind::Stop]),
                "<Restrictions><Type>stop</Type><NumberOfResults>5</NumberOfResults></Restrictions>",
            ),
        ] {
            let expected = envelope(
                protocol,
                "LocationInformationRequest",
                &format!(
                    "<InitialInput><LocationName>Hauptbahnhof</LocationName></InitialInput>
                     {restrictions}"
                ),
            );
            let request = network(protocol).locations_request(&query).unwrap();
            assert_eq!(body_mismatch(&request, &expected), None, "{protocol:?}");
        }
    }
}

#[test]
fn departures_request_asks_for_realtime_departures_and_omits_what_the_query_leaves_open() {
    let query = DeparturesQuery::new("de:14612:28");
    for protocol in PROTOCOLS {
        for (query, time, limit) in [
            (query.clone(), "", ""),
            (
                query.clone().with_time(utc(13, 24, 0)).with_limit(10),
                "<DepArrTime>2026-11-16T13:24:00Z</DepArrTime>",
                "<NumberOfResults>10</NumberOfResults>",
            ),
        ] {
            let stop = stop_ref(protocol);
            let expected = envelope(
                protocol,
                "StopEventRequest",
                &format!(
                    "<Location>{stop}{time}</Location>
                     <Params>
                        {limit}
                        <StopEventType>departure</StopEventType>
                        <IncludeRealtimeData>true</IncludeRealtimeData>
                     </Params>"
                ),
            );
            let request = network(protocol).departures_request(&query).unwrap();
            assert_eq!(body_mismatch(&request, &expected), None, "{protocol:?}");
        }
    }
}

#[test]
fn trips_request_puts_the_time_at_the_end_it_binds_and_names_stops_and_positions() {
    let query = TripsQuery::new(
        Place::Id("de:14612:28".to_owned()),
        Place::Coordinate(Coordinate {
            latitude: 51.0633,
            longitude: 13.7467,
        }),
    );
    let time = "<DepArrTime>2026-11-16T13:24:00Z</DepArrTime>";
    for protocol in PROTOCOLS {
        for (query, origin_time, destination_time, limit) in [
            (query.clone(), "", "", ""),
            (
                query.clone().with_departure(utc(13, 24, 0)).with_limit(3),
                time,
                "",
                "<NumberOfResultsBefore>0</NumberOfResultsBefore><NumberOfResultsAfter>3</NumberOfResultsAfter>",
            ),
            // A plain `NumberOfResults` makes the servers add a trip from
            // the other side of the time. The schemas want both counts.
            (
                query.clone().with_arrival(utc(13, 24, 0)).with_limit(3),
                "",
                time,
                "<NumberOfResultsBefore>3</NumberOfResultsBefore><NumberOfResultsAfter>0</NumberOfResultsAfter>",
            ),
        ] {
            let (stop, position) = (stop_ref(protocol), position_ref(protocol));
            let expected = envelope(
                protocol,
                "TripRequest",
                &format!(
                    "<Origin>{stop}{origin_time}</Origin>
                     <Destination>{position}{destination_time}</Destination>
                     <Params>
                        {limit}
                        <IncludeIntermediateStops>true</IncludeIntermediateStops>
                     </Params>"
                ),
            );
            let request = network(protocol).trips_request(&query).unwrap();
            assert_eq!(body_mismatch(&request, &expected), None, "{protocol:?}");
        }
    }
}

/// OJP has the locality in parentheses after the full name.
#[test]
fn locations_have_a_locality_and_the_time_zone_of_the_network() {
    let parse = |protocol, body| {
        network(protocol)
            .with_timezone("Europe/Berlin")
            .parse_locations(&response(body))
            .unwrap()
    };

    let stop = parse(Protocol::Trias, TRIAS_LOCATIONS);
    let expected: LocationFields = (
        "de:14612:28",
        vec![ifopt("de:14612:28")],
        LocationKind::Stop,
        "Hauptbahnhof",
        Some("Dresden"),
        Some((51.03993, 13.73293)),
    );
    assert_eq!(stop.iter().map(fields).collect::<Vec<_>>(), [expected]);

    let pois = parse(Protocol::Trias, TRIAS_NO_MATCH);
    let expected: LocationFields = (
        "poiID:40686952:3401000:117023967:Oxzy:Mitte:Oxzy:ANY:POI:961025:5007911:MRCV:VVO",
        Vec::new(),
        LocationKind::Poi,
        "Mitte, Oxzy",
        Some("Mitte"),
        Some((53.04912, 8.63303)),
    );
    assert_eq!(pois.len(), 3);
    assert_eq!(fields(&pois[0]), expected);

    let ojp = parse(Protocol::Ojp1, OJP_LOCATIONS);
    let expected: [LocationFields; 3] = [
        (
            "de:05111:18235",
            vec![ifopt("de:05111:18235")],
            LocationKind::Stop,
            "Düsseldorf Hbf",
            Some("Düsseldorf"),
            Some((51.22025, 6.793)),
        ),
        (
            "poiID:302375:5111000:-1:Hauptbahnhof Düsseldorf:Düsseldorf:Hauptbahnhof Düsseldorf:ANY:POI:756403:5339904:MRCV:nrw",
            Vec::new(),
            LocationKind::Poi,
            "Düsseldorf, Hauptbahnhof Düsseldorf",
            Some("Düsseldorf"),
            Some((51.21884, 6.79488)),
        ),
        (
            "poiID:287117:5111000:-1:B&R, Düsseldorf Hauptbahnhof:Düsseldorf:B&R, Düsseldorf Hauptbahnhof:ANY:POI:756459:5339889:MRCV:nrw",
            Vec::new(),
            LocationKind::Poi,
            "Düsseldorf, B&R, Düsseldorf Hauptbahnhof",
            Some("Düsseldorf"),
            Some((51.21893, 6.79539)),
        ),
    ];
    assert_eq!(ojp.len(), 4);
    assert_eq!(ojp.iter().take(3).map(fields).collect::<Vec<_>>(), expected);

    for location in stop.iter().chain(&pois).chain(&ojp) {
        assert_eq!(location.timezone.as_deref(), Some("Europe/Berlin"));
    }
    let without_timezone = network(Protocol::Trias).parse_locations(&response(TRIAS_LOCATIONS));
    assert_eq!(without_timezone.unwrap()[0].timezone, None);
}

#[test]
fn locations_are_told_apart_by_their_element_and_ordered_by_probability() {
    for (protocol, fixture, other, best) in [
        (Protocol::Trias, TRIAS_NO_MATCH, "Locality", "0.337000012"),
        (Protocol::Ojp1, OJP_LOCATIONS, "TopographicPlace", "1"),
    ] {
        let parse = |body: &str| {
            network(protocol)
                .parse_locations(&response(body.as_bytes()))
                .unwrap()
        };
        let recorded = parse(&text(fixture));
        let pois = || {
            recorded
                .iter()
                .filter(|location| location.kind == LocationKind::Poi)
        };

        // An address has the same parts as a point of interest, under its
        // own names.
        let addresses = parse(&text(fixture).replace("PointOfInterest", "Address"));
        assert_eq!(
            addresses
                .iter()
                .filter(|location| location.kind == LocationKind::Address)
                .map(|location| (&location.id, &location.name, location.global_ids.as_slice()))
                .collect::<Vec<_>>(),
            pois()
                .map(|location| (&location.id, &location.name, &[] as &[GlobalId]))
                .collect::<Vec<_>>(),
            "{protocol:?}"
        );

        let others = parse(&text(fixture).replace("PointOfInterest", other));
        assert_eq!(others.len(), recorded.len(), "{protocol:?}");
        assert_eq!(
            others
                .iter()
                .filter(|location| location.kind == LocationKind::Other)
                .map(|location| location.coordinate)
                .collect::<Vec<_>>(),
            pois()
                .map(|location| location.coordinate)
                .collect::<Vec<_>>(),
            "{protocol:?}"
        );

        // The best match becomes the worst one.
        let (p, _) = prefixes(protocol);
        let from = format!("<{p}:Probability>{best}</{p}:Probability>");
        let to = format!("<{p}:Probability>0.125</{p}:Probability>");
        assert!(text(fixture).contains(&from));
        let reordered = parse(&text(fixture).replacen(&from, &to, 1));
        assert_eq!(reordered.len(), recorded.len(), "{protocol:?}");
        assert_eq!(
            reordered.last().map(fields),
            recorded.first().map(fields),
            "{protocol:?}"
        );
    }
}

#[test]
fn both_protocols_describe_the_same_departures_alike_and_order_them_by_expected_time() {
    // Ordered by expected time, then by stop id. The second one is a bus that
    // replaces a train and has no prediction, the last one changed platform.
    let expected = [
        "22:28:00 22:28:00 de:05111:18235:2:9 9/9 Düsseldorf Hbf Bus D-Flughafen Terminal S U",
        "22:28:00 - de:05111:18235:94:17 17/17 Düsseldorf Hbf Bus Essen Hauptbahnhof",
        "22:30:00 22:30:00 de:05111:18235:98:13 13/13 Düsseldorf Hbf SuburbanTrain Hagen Hbf",
        "22:31:30 22:31:30 de:05111:18235:3:14 14/14 Düsseldorf Hbf Bus D-Kirchplatz U",
        "22:23:00 22:34:00 de:05111:18235:98:13 13/13 Düsseldorf Hbf SuburbanTrain D-Flughafen Terminal S U",
        "22:31:30 22:34:12 de:05111:18235:6:3 3/3 Düsseldorf Hbf Subway D-Heinrich-Heine-Allee U",
        "22:25:00 22:42:00 de:05111:18235:93:16 16/16 Düsseldorf Hbf HighSpeedTrain München Hbf",
        "20:10:00 22:49:00 de:05111:18235:93:15 15/15 Düsseldorf Hbf HighSpeedTrain Köln Hbf",
        "21:12:00 22:56:00 de:05111:18235:93:15 15/15 Düsseldorf Hbf HighSpeedTrain Köln Hbf",
        "22:21:00 22:57:00 de:05111:18235:90:4 4/10 Düsseldorf Hbf RegionalTrain Aachen, Hbf",
    ];
    for (protocol, body) in [(Protocol::Trias, TRIAS_VRR), (Protocol::Ojp1, OJP_VRR)] {
        let departures = network(protocol).parse_departures(&response(body)).unwrap();
        assert!(
            departures.is_sorted_by_key(Departure::expected_time),
            "{protocol:?}"
        );
        let mut by_time_and_stop: Vec<_> = departures.iter().collect();
        by_time_and_stop.sort_by_key(|departure| (departure.expected_time(), &departure.stop.id));
        assert_eq!(
            by_time_and_stop
                .iter()
                .map(|departure| summary(departure))
                .collect::<Vec<_>>(),
            expected,
            "{protocol:?}"
        );
        for departure in &departures {
            let stop = &departure.stop;
            assert_eq!(
                (stop.kind, &stop.locality, stop.coordinate),
                (LocationKind::Stop, &None, None),
                "{protocol:?}"
            );
            assert_eq!(stop.global_ids, [ifopt(&stop.id)], "{protocol:?}");
            assert_eq!(
                stop.station_ids(),
                [ifopt("de:05111:18235")],
                "{protocol:?}"
            );
            assert!(!departure.cancelled, "{protocol:?}");
        }
    }
}

#[test]
fn ojp_names_trains_without_a_line_by_category_and_number_and_knows_the_operator() {
    // Scheduled time, then name and product in TRIAS and name, product and
    // operator in OJP.
    let expected = [
        (
            utc(20, 10, 0),
            ("ICE 544 InterCityExpress", "ICE 544 InterCityExpress"),
            ("ICE 544", "Zug", "DB Fernverkehr AG"),
        ),
        (
            utc(22, 21, 0),
            ("RE 4", "RE 26438 Wupper-Express"),
            ("RE 4", "Regionalzug", "National Express"),
        ),
        (
            utc(22, 23, 0),
            ("S 11", "S-Bahn"),
            ("S 11", "S-Bahn", "DB AG"),
        ),
    ];
    let line = |protocol, body, scheduled| {
        let departures = network(protocol).parse_departures(&response(body)).unwrap();
        let departure = departures
            .iter()
            .find(|departure| departure.scheduled_time == scheduled)
            .unwrap();
        departure.line.clone()
    };
    for (scheduled, (name, product), (ojp_name, ojp_product, operator)) in expected {
        let trias = line(Protocol::Trias, TRIAS_VRR, scheduled);
        assert_eq!(
            (
                trias.name.as_str(),
                trias.product.as_deref(),
                trias.operator
            ),
            (name, Some(product), None)
        );
        let ojp = line(Protocol::Ojp1, OJP_VRR, scheduled);
        assert_eq!(
            (
                ojp.name.as_str(),
                ojp.product.as_deref(),
                ojp.operator.as_deref()
            ),
            (ojp_name, Some(ojp_product), Some(operator))
        );
    }
}

#[test]
fn mode_follows_the_pt_mode_and_for_trains_the_submode() {
    let modes = [
        ("rail", Some("longDistance"), Mode::LongDistanceTrain),
        ("rail", Some("interregionalRail"), Mode::LongDistanceTrain),
        ("rail", Some("international"), Mode::LongDistanceTrain),
        ("rail", Some("nightRail"), Mode::NightTrain),
        ("rail", Some("sleeperRailService"), Mode::NightTrain),
        ("rail", Some("regionalRail"), Mode::RegionalTrain),
        ("rail", None, Mode::RegionalTrain),
        ("urbanRail", None, Mode::SuburbanTrain),
        ("tram", None, Mode::Tram),
        ("trolleyBus", None, Mode::Bus),
        ("coach", None, Mode::Coach),
        ("water", None, Mode::Ferry),
        ("telecabin", None, Mode::CableCar),
        ("cableway", None, Mode::CableCar),
        ("funicular", None, Mode::Funicular),
        ("air", None, Mode::Airplane),
        ("taxi", None, Mode::OnDemand),
        ("unknown", None, Mode::Other),
    ];
    for (protocol, fixture, high_speed) in [
        (Protocol::Trias, TRIAS_VRR, "highSpeedRail"),
        (Protocol::Ojp1, OJP_VRR, "highSpeedRailService"),
    ] {
        // The mode of the first departure in the response, an ICE.
        let (p, s) = prefixes(protocol);
        let submode = |name: &str| format!("<{s}:RailSubmode>{name}</{s}:RailSubmode>");
        let recorded = format!("<{p}:PtMode>rail</{p}:PtMode>{}", submode(high_speed));
        assert!(text(fixture).contains(&recorded));
        for (pt_mode, rail_submode, mode) in modes {
            let replaced = format!(
                "<{p}:PtMode>{pt_mode}</{p}:PtMode>{}",
                rail_submode.map_or_default(submode)
            );
            let body = text(fixture).replacen(&recorded, &replaced, 1);
            let departures = network(protocol)
                .parse_departures(&response(body.as_bytes()))
                .unwrap();
            let ice = departures
                .iter()
                .find(|departure| departure.scheduled_time == utc(20, 10, 0))
                .unwrap();
            assert_eq!(
                ice.line.mode, mode,
                "{protocol:?} {pt_mode} {rail_submode:?}"
            );
        }
    }
}

#[test]
fn cancelled_service_is_a_cancelled_departure() {
    for (protocol, fixture) in [(Protocol::Trias, TRIAS_VRR), (Protocol::Ojp1, OJP_VRR)] {
        // The first departure in the response, scheduled for 20:10.
        let (p, _) = prefixes(protocol);
        let deviation = format!("<{p}:Deviation>true</{p}:Deviation>");
        let cancelled = format!("<{p}:Cancelled>true</{p}:Cancelled>{deviation}");
        assert!(text(fixture).contains(&deviation));
        let body = text(fixture).replacen(&deviation, &cancelled, 1);
        let departures = network(protocol)
            .parse_departures(&response(body.as_bytes()))
            .unwrap();
        assert_eq!(
            departures
                .iter()
                .filter(|departure| departure.cancelled)
                .map(|departure| departure.scheduled_time)
                .collect::<Vec<_>>(),
            [utc(20, 10, 0)],
            "{protocol:?}"
        );
    }
}

#[test]
fn error_without_results_is_an_empty_list_not_found_or_a_server_error() {
    let outcome = |protocol, kind, body: &str| match count(protocol, kind, body.as_bytes()) {
        Ok(results) => format!("{results} results"),
        Err(Error::NotFound { .. }) => "not found".to_owned(),
        Err(Error::Server { message, .. }) => format!("server: {message}"),
        Err(error) => format!("{error:?}"),
    };
    for protocol in PROTOCOLS {
        for (kind, message, expected) in [
            ("LocationInformation", "LOCATION_NORESULTS", "0 results"),
            ("StopEvent", "STOPEVENT_NOEVENTFOUND", "0 results"),
            ("StopEvent", "STOPEVENT_LOCATIONUNSERVED", "not found"),
            ("StopEvent", "STOPEVENT_LOCATIONUNKNOWN", "not found"),
            (
                "StopEvent",
                "STOPEVENT_INTERNAL_ERROR",
                "server: STOPEVENT_INTERNAL_ERROR",
            ),
            ("Trip", "TRIP_NOTRIPFOUND", "0 results"),
            ("Trip", "TRIP_ORIGINDESTINATIONIDENTICAL", "0 results"),
            ("Trip", "TRIP_ORIGINUNKNOWN", "not found"),
            ("Trip", "TRIP_DESTINATIONUNKNOWN", "not found"),
        ] {
            assert_eq!(
                outcome(protocol, kind, &error_body(protocol, kind, message)),
                expected,
                "{protocol:?} {message}"
            );
        }
    }

    // The same error next to results does not hide them.
    let trias = text(TRIAS_VVO).replacen(
        "<trias:StopEventResponse>",
        "<trias:StopEventResponse><trias:ErrorMessage><trias:Code>-100</trias:Code>\
         <trias:Text><trias:Text>STOPEVENT_LOCATIONUNSERVED</trias:Text>\
         <trias:Language>de</trias:Language></trias:Text></trias:ErrorMessage>",
        1,
    );
    assert_eq!(outcome(Protocol::Trias, "StopEvent", &trias), "10 results");
    let ojp = text(OJP_VRR).replacen(
        "<siri:Status>true</siri:Status><ojp:CalcTime>",
        "<siri:Status>true</siri:Status><siri:ErrorCondition><siri:OtherError>\
         <siri:ErrorText>STOPEVENT_LOCATIONUNSERVED</siri:ErrorText></siri:OtherError>\
         <siri:Description>STOPEVENT_LOCATIONUNSERVED</siri:Description>\
         </siri:ErrorCondition><ojp:CalcTime>",
        1,
    );
    assert_ne!(ojp, text(OJP_VRR));
    assert_eq!(outcome(Protocol::Ojp1, "StopEvent", &ojp), "10 results");
}

#[test]
fn trias_reads_rides_with_their_calls_and_the_walk_between_stops() {
    let trips = network(Protocol::Trias)
        .parse_trips(&response(TRIAS_TRIP))
        .unwrap()
        .trips;
    let legs: Vec<Vec<_>> = trips
        .iter()
        .map(|trip| trip.legs.iter().map(label).collect())
        .collect();
    assert_eq!(
        legs,
        [
            vec!["7"],
            vec!["10", "walk"],
            vec!["3"],
            vec!["S1", "7"],
            vec!["8"]
        ]
    );

    let [tram, walk] = trips[1].legs.as_slice() else {
        panic!("{:?}", trips[1]);
    };
    assert_eq!(
        ride(tram),
        Some((
            "10",
            Mode::Tram,
            Some("Straßenbahn"),
            None,
            Some("Btf. Trachenberge")
        ))
    );
    assert_eq!(
        end(&tram.departure),
        "22:25:00 22:28:18 de:14612:28:3:2 2/2 Dresden Hauptbahnhof"
    );
    assert_eq!(tram.departure.stop.kind, LocationKind::Stop);
    assert_eq!(tram.departure.stop.station_ids(), [ifopt("de:14612:28")]);
    assert_eq!(
        end(&tram.arrival),
        "22:35:00 22:38:18 de:14612:17:1:4 4/4 Dresden Anton-/Leipziger Straße"
    );
    assert_eq!(
        tram.intermediate_stops
            .iter()
            .map(|call| call.stop.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Dresden Budapester Straße",
            "Dresden S-Bf. Freiberger Straße",
            "Dresden Bahnhof Mitte",
            "Dresden Kongresszentrum / Haus der Presse",
        ]
    );

    assert_eq!(walk.kind, LegKind::Walk);
    assert_eq!(
        end(&walk.departure),
        "22:38:18 - de:14612:17:1:4 -/- Dresden Anton-/Leipziger Straße"
    );
    assert_eq!(
        end(&walk.arrival),
        "22:51:18 - de:14612:13 -/- Dresden Albertplatz"
    );
    assert_eq!(walk.intermediate_stops, []);
}

#[test]
fn ojp_reads_rides_with_their_calls_operator_and_missing_platforms() {
    let trips = network(Protocol::Ojp1)
        .parse_trips(&response(OJP_TRIP))
        .unwrap()
        .trips;
    let legs: Vec<Vec<_>> = trips
        .iter()
        .map(|trip| trip.legs.iter().map(label).collect())
        .collect();
    assert_eq!(legs, [["RE 1 (RRX)"], ["RE 1 (RRX)"], ["ICE 100"]]);

    let regional = &trips[0].legs[0];
    assert_eq!(
        ride(regional),
        Some((
            "RE 1 (RRX)",
            Mode::RegionalTrain,
            Some("Regionalzug"),
            Some("National Express"),
            Some("Bochum Hbf"),
        ))
    );
    assert_eq!(
        end(&regional.departure),
        "22:22:00 22:27:00 de:05111:18235:94:17 17/17 Düsseldorf Hbf"
    );
    assert_eq!(
        end(&regional.arrival),
        "22:50:00 22:54:00 de:05113:9289:91:6 6/6 Essen Hauptbahnhof"
    );
    let calls = &regional.intermediate_stops;
    assert_eq!(
        calls
            .iter()
            .map(|call| call.stop.name.as_str())
            .collect::<Vec<_>>(),
        ["D-Flughafen Bf", "Duisburg Hbf", "Mülheim Hauptbahnhof"]
    );
    assert_eq!(calls[0].stop.id, "de:05111:18488:90:1");
    assert_eq!(
        times(&calls[0]),
        ["22:27:00", "22:32:00", "22:28:00", "22:33:00"].map(|time| Some(time.to_owned()))
    );

    // The server sends no line and no platform for this train.
    let ice = &trips[2].legs[0];
    assert_eq!(
        ride(ice),
        Some((
            "ICE 100",
            Mode::HighSpeedTrain,
            Some("Zug"),
            Some("DB Fernverkehr AG"),
            Some("Dortmund Hbf"),
        ))
    );
    assert_eq!(
        end(&ice.departure),
        "22:40:00 23:28:00 de:05111:18235:94 -/- Düsseldorf Hbf"
    );
}

#[test]
fn leg_without_a_vehicle_is_a_walk_only_if_its_mode_says_so() {
    // Element of the leg, path of its mode and a mode other than `walk`.
    let trias_legs = [
        ("InterchangeLeg", "InterchangeMode", "parkAndRide"),
        ("ContinuousLeg", "ContinuousMode", "demandResponsive"),
    ];
    let ojp_legs = [
        ("TransferLeg", "TransferMode", "parkAndRide"),
        ("ContinuousLeg", "Service/IndividualMode", "cycle"),
    ];
    for (protocol, legs) in [(Protocol::Trias, trias_legs), (Protocol::Ojp1, ojp_legs)] {
        for (leg, mode, other) in legs {
            for (value, kind) in [("walk", LegKind::Walk), (other, LegKind::Other)] {
                let body = trips_with_leg(protocol, leg, mode, value);
                let trips = network(protocol)
                    .parse_trips(&response(body.as_bytes()))
                    .unwrap()
                    .trips;
                let added = trips.last().unwrap().legs.last().unwrap();
                assert_eq!(added.kind, kind, "{protocol:?} {leg} {value}");
                assert_eq!(
                    [end(&added.departure), end(&added.arrival)],
                    [
                        "23:54:00 - de:05113:9289:91 -/- Essen Hauptbahnhof",
                        "23:59:30 - de:05113:9143 -/- Essen Rathaus",
                    ],
                    "{protocol:?} {leg}"
                );
            }
        }
    }
}

#[test]
fn success_status_with_a_foreign_body_is_a_parse_error() {
    for (protocol, other) in [
        (Protocol::Trias, Protocol::Ojp1),
        (Protocol::Ojp1, Protocol::Trias),
    ] {
        for kind in KINDS {
            for body in [
                b"<!DOCTYPE html><html><body>Service unavailable</body></html>".as_slice(),
                b"",
                b"{}",
                fixture(other, kind),
            ] {
                let result = count(protocol, kind, body);
                assert!(
                    matches!(result, Err(Error::Parse { .. })),
                    "{protocol:?} {kind} {result:?}"
                );
            }
        }
    }
}

/// A TRIAS response around `delivery`, and an OJP one around `delivery`.
fn trias_response(delivery: &str) -> String {
    format!(
        r#"<trias:Trias xmlns:siri="http://www.siri.org.uk/siri" xmlns:trias="http://www.vdv.de/trias" version="1.2"><trias:ServiceDelivery>{delivery}</trias:ServiceDelivery></trias:Trias>"#
    )
}

fn ojp_response(delivery: &str) -> String {
    format!(
        r#"<siri:OJP xmlns:siri="http://www.siri.org.uk/siri" xmlns:ojp="http://www.vdv.de/ojp" version="1.0"><siri:OJPResponse><siri:ServiceDelivery>{delivery}</siri:ServiceDelivery></siri:OJPResponse></siri:OJP>"#
    )
}

/// An error must not look like a stop without departures, wherever the
/// server puts it and whether or not it has a text.
#[test]
fn errors_are_read_at_every_level_and_without_a_text() {
    let outcome = |protocol, body: String| match count(protocol, "StopEvent", body.as_bytes()) {
        Ok(results) => format!("{results} results"),
        Err(Error::NotFound { message }) => format!("not found: {message}"),
        Err(Error::Server { message, .. }) => format!("server: {message}"),
        Err(error) => format!("{error:?}"),
    };
    let no_text = "server: the server reported an error without a text";
    let trias = |answer: &str| {
        trias_response(&format!(
            "<trias:DeliveryPayload><trias:StopEventResponse>{answer}</trias:StopEventResponse></trias:DeliveryPayload>"
        ))
    };
    let message = |text: &str| {
        format!(
            "<trias:ErrorMessage><trias:Code>-1</trias:Code><trias:Text><trias:Text>{text}</trias:Text></trias:Text></trias:ErrorMessage>"
        )
    };
    assert_eq!(
        outcome(
            Protocol::Trias,
            trias("<trias:ErrorMessage><trias:Code>-100</trias:Code></trias:ErrorMessage>")
        ),
        no_text
    );
    // "Nothing found" does not hide the reason next to it.
    assert_eq!(
        outcome(
            Protocol::Trias,
            trias(&(message("STOPEVENT_NOEVENTFOUND") + &message("\n STOPEVENT_LOCATIONUNKNOWN ")))
        ),
        "not found: STOPEVENT_LOCATIONUNKNOWN"
    );
    // A text that is not an error code is not matched by its ending.
    assert_eq!(
        outcome(
            Protocol::Trias,
            trias(&message("Internal error, cause UNKNOWN"))
        ),
        "server: Internal error, cause UNKNOWN"
    );
    // The envelope reports the error and holds no answer at all.
    let envelope = "<siri:Status>false</siri:Status><siri:ErrorCondition><siri:OtherError><siri:ErrorText>Requestor not allowed</siri:ErrorText></siri:OtherError></siri:ErrorCondition>";
    assert_eq!(
        outcome(Protocol::Trias, trias_response(envelope)),
        "server: Requestor not allowed"
    );
    assert_eq!(
        outcome(Protocol::Ojp1, ojp_response(envelope)),
        "server: Requestor not allowed"
    );

    let ojp = |answer: &str| {
        ojp_response(&format!(
            "<siri:Status>true</siri:Status><ojp:OJPStopEventDelivery>{answer}</ojp:OJPStopEventDelivery>"
        ))
    };
    // The text is only inside the element that names the kind of error.
    assert_eq!(
        outcome(
            Protocol::Ojp1,
            ojp(
                "<siri:Status>false</siri:Status><siri:ErrorCondition><siri:OtherError><siri:ErrorText>STOPEVENT_LOCATIONUNKNOWN</siri:ErrorText></siri:OtherError></siri:ErrorCondition>"
            )
        ),
        "not found: STOPEVENT_LOCATIONUNKNOWN"
    );
    assert_eq!(
        outcome(Protocol::Ojp1, ojp("<siri:Status>false</siri:Status>")),
        no_text
    );
    assert_eq!(
        outcome(Protocol::Ojp1, ojp("<siri:Status>true</siri:Status>")),
        "0 results"
    );
}

/// The schemas allow a name per language, and whitespace around values.
#[test]
fn repeated_names_whitespace_and_a_skipped_stop_are_read() {
    let name = |element: &str, text: &str| {
        format!(
            "<trias:{element}><trias:Text>{text}</trias:Text><trias:Language>de</trias:Language></trias:{element}>"
        )
    };
    let body = trias_response(&format!(
        "<trias:DeliveryPayload><trias:StopEventResponse><trias:StopEventResult><trias:StopEvent>\
         <trias:ThisCall><trias:CallAtStop><trias:StopPointRef>\n de:14612:28 \n</trias:StopPointRef>{}{}{}\
         <trias:ServiceDeparture><trias:TimetabledTime> 2026-11-16T13:24:00Z </trias:TimetabledTime></trias:ServiceDeparture>\
         <trias:NotServicedStop>true</trias:NotServicedStop></trias:CallAtStop></trias:ThisCall>\
         <trias:Service><trias:ServiceSection><trias:Mode><trias:PtMode> tram\n</trias:PtMode></trias:Mode>{}{}</trias:ServiceSection>{}{}</trias:Service>\
         </trias:StopEvent></trias:StopEventResult></trias:StopEventResponse></trias:DeliveryPayload>",
        name("StopPointName", "Hauptbahnhof"),
        name("StopPointName", "Main station"),
        name("PlannedBay", "2"),
        name("PublishedLineName", "7"),
        name("PublishedLineName", "Seven"),
        name("DestinationText", "Pennrich"),
        name("DestinationText", "Pennrich (en)"),
    ));
    let departures = network(Protocol::Trias)
        .parse_departures(&response(body.as_bytes()))
        .unwrap();
    let departure = &departures[0];
    assert_eq!(departure.stop.id, "de:14612:28");
    assert_eq!(departure.stop.name, "Hauptbahnhof");
    assert_eq!(departure.line.name, "7");
    assert_eq!(departure.line.mode, Mode::Tram);
    assert_eq!(departure.direction.as_deref(), Some("Pennrich"));
    assert_eq!(departure.scheduled_time.unix_seconds, 1_794_835_440);
    assert!(departure.cancelled, "the vehicle does not call at the stop");
}

/// The locality is in the last parentheses of the full name, which can hold
/// parentheses of their own.
#[test]
fn ojp_locality_is_the_last_parenthesized_part_of_the_name() {
    let location = |name: &str| {
        let body = ojp_response(&format!(
            "<ojp:OJPLocationInformationDelivery><ojp:Location><ojp:Location><ojp:StopPlace>\
             <ojp:StopPlaceRef>de:1:2</ojp:StopPlaceRef><ojp:StopPlaceName><ojp:Text>x</ojp:Text></ojp:StopPlaceName>\
             </ojp:StopPlace><ojp:LocationName><ojp:Text>{name}</ojp:Text></ojp:LocationName></ojp:Location></ojp:Location>\
             </ojp:OJPLocationInformationDelivery>"
        ));
        network(Protocol::Ojp1)
            .parse_locations(&response(body.as_bytes()))
            .unwrap()
            .swap_remove(0)
            .locality
    };
    assert_eq!(
        location("Bahnhof (Halle (Westf.))").as_deref(),
        Some("Halle (Westf.)")
    );
    assert_eq!(
        location("Hbf (tief) (Frankfurt)").as_deref(),
        Some("Frankfurt")
    );
    assert_eq!(location("Platz ()"), None);
    assert_eq!(location("Hauptbahnhof"), None);
}

/// XML 1.0 has no way to write most control characters.
#[test]
fn text_that_xml_cannot_carry_is_an_invalid_query() {
    for protocol in PROTOCOLS {
        let network = network(protocol);
        let results = [
            network.locations_request(&LocationQuery::new("a\u{1}b")),
            network.departures_request(&DeparturesQuery::new("a\u{0}")),
            network.trips_request(&TripsQuery::new(
                Place::Id("\u{FFFF}".to_owned()),
                Place::Id("b".to_owned()),
            )),
        ];
        for result in results {
            assert!(
                matches!(result, Err(Error::InvalidQuery { .. })),
                "{result:?}"
            );
        }
    }
}
