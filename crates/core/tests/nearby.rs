//! The stops near a position, for every protocol.

use proptest::prelude::*;
use publictransport::{
    Coordinate, Error, HafasOptions, HttpRequest, HttpResponse, Location, LocationKind, Method,
    NearbyQuery, Network, Operation, Protocol,
};
use serde_json::{Value, json};

const ENDPOINT: &str = "https://example.org/base";

const PROTOCOLS: [Protocol; 6] = [
    Protocol::Efa,
    Protocol::Motis,
    Protocol::Trias,
    Protocol::Ojp1,
    Protocol::HafasMgate,
    Protocol::OtpTransmodel,
];

/// A position at Dresden Hauptbahnhof.
const DRESDEN: Coordinate = Coordinate {
    latitude: 51.04,
    longitude: 13.7325,
};

/// A network with what its protocol needs to build a request. The one for
/// `OpenTripPlanner` has no geocoder.
fn network(protocol: Protocol) -> Network {
    let network = Network::new("test", "Test", ENDPOINT, protocol);
    match protocol {
        Protocol::Trias | Protocol::Ojp1 => network.with_requestor_ref("OpenService"),
        Protocol::HafasMgate => network
            .with_hafas(HafasOptions::new(
                r#"{"type":"WEB","id":"VBB","name":"VBB WebApp","l":"vs_webapp_vbb"}"#,
                r#"{"type":"AID","aid":"hafas-vbb-webapp"}"#,
                "1.33",
            ))
            .with_timezone("Europe/Berlin"),
        _ => network,
    }
}

fn respond(status: u16, body: &[u8]) -> HttpResponse {
    HttpResponse {
        status,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

/// The decoded values of every query parameter called `name`.
fn parameter(request: &HttpRequest, name: &str) -> Vec<String> {
    let query = request.url.split_once('?').map_or("", |(_, query)| query);
    form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// The parts of a text as numbers, `None` for a part that is none.
fn numbers(text: &str, separator: char) -> Vec<Option<f64>> {
    text.split(separator)
        .map(|part| part.parse().ok())
        .collect()
}

/// The body as JSON, `null` if it is none.
fn body(request: &HttpRequest) -> Value {
    serde_json::from_slice(request.body.as_deref().unwrap_or_default()).unwrap_or_default()
}

/// Every element of an XML body that has no child elements: the names of
/// its ancestors and its own, joined by `/`, and its text.
fn leaves(request: &HttpRequest) -> Vec<(String, String)> {
    let body = std::str::from_utf8(request.body.as_deref().unwrap_or_default()).unwrap_or_default();
    let document = roxmltree::Document::parse(body);
    document
        .iter()
        .flat_map(roxmltree::Document::descendants)
        .filter(|node| node.is_element() && !node.children().any(|child| child.is_element()))
        .map(|leaf| {
            let mut path: Vec<_> = leaf
                .ancestors()
                .filter(roxmltree::Node::is_element)
                .map(|node| node.tag_name().name())
                .collect();
            path.reverse();
            (path.join("/"), leaf.text().unwrap_or_default().to_owned())
        })
        .collect()
}

/// The number an argument of the GraphQL field `nearest` carries, whether
/// it stands in the query or in a variable.
fn nearest_argument(request: &HttpRequest, name: &str) -> Option<f64> {
    let body = body(request);
    let (_, after) = body.get("query")?.as_str()?.split_once("nearest")?;
    let arguments = after.split(')').next()?;
    let (_, value) = arguments.split_once(&format!("{name}:"))?;
    let value = value
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()?;
    match value.strip_prefix('$') {
        Some(variable) => body.get("variables")?.get(variable)?.as_f64(),
        None => value.parse().ok(),
    }
}

#[test]
fn request_carries_the_position_the_radius_and_the_limit_if_there_is_one() {
    let plain = NearbyQuery::new(DRESDEN);
    let detailed = plain.clone().with_radius_meters(437).with_limit(7);
    let (latitude, longitude) = (Some(DRESDEN.latitude), Some(DRESDEN.longitude));

    for protocol in PROTOCOLS {
        // An EFA server, and with it the TRIAS and OJP servers built on one,
        // measures the radius on a Mercator map: in Dresden its meters are
        // 0.63 real ones, so the radius is sent stretched.
        for (query, radius, stretched, limit) in [
            (&plain, 1000_u32, 1591_u32, None),
            (&detailed, 437, 695, Some(7_u32)),
        ] {
            let case = format!("{protocol:?}, {query:?}");
            let request = network(protocol).nearby_request(query).unwrap();
            let stretched_number = Some(f64::from(stretched));
            let radius_number = Some(f64::from(radius));
            let limit_number = limit.map(f64::from);
            let limit_text: Vec<String> = limit.iter().map(ToString::to_string).collect();
            let one = |name| parameter(&request, name).join("|");

            match protocol {
                Protocol::Efa => {
                    assert_eq!(request.method, Method::Get, "{case}");
                    let prefix = format!("{ENDPOINT}/XML_COORD_REQUEST?");
                    assert!(request.url.starts_with(&prefix), "{}", request.url);
                    let coord = one("coord");
                    assert_eq!(numbers(&coord, ':'), [longitude, latitude, None], "{coord}");
                    assert!(coord.ends_with(":WGS84[dd.ddddd]"), "{coord}");
                    assert_eq!(one("inclFilter"), "1", "{case}");
                    assert_eq!(one("type_1"), "STOP", "{case}");
                    assert_eq!(one("radius_1"), stretched.to_string(), "{case}");
                    assert_eq!(parameter(&request, "max"), limit_text, "{case}");
                    assert_eq!(one("outputFormat"), "rapidJSON", "{case}");
                    assert_eq!(one("coordOutputFormat"), "WGS84[dd.ddddd]", "{case}");
                }
                Protocol::Motis => {
                    assert_eq!(request.method, Method::Get, "{case}");
                    let prefix = format!("{ENDPOINT}/v1/reverse-geocode?");
                    assert!(request.url.starts_with(&prefix), "{}", request.url);
                    assert_eq!(numbers(&one("place"), ','), [latitude, longitude], "{case}");
                    assert_eq!(one("type"), "STOP", "{case}");
                    assert_eq!(parameter(&request, "numResults"), limit_text, "{case}");
                    // The server knows no radius.
                    let parameters = 2 + limit_text.len();
                    assert_eq!(request.url.matches('=').count(), parameters, "{case}");
                }
                Protocol::Trias | Protocol::Ojp1 => {
                    assert_eq!(request.method, Method::Post, "{case}");
                    assert_eq!(request.url, ENDPOINT, "{case}");
                    let leaves = leaves(&request);
                    let number = |path: &str| {
                        let path = format!("LocationInformationRequest/{path}");
                        let mut found = leaves.iter().filter(|(leaf, _)| leaf.ends_with(&path));
                        found.next().and_then(|(_, text)| text.parse::<f64>().ok())
                    };
                    let circle = "InitialInput/GeoRestriction/Circle";
                    let center = |name| number(&format!("{circle}/Center/{name}"));
                    assert_eq!(center("Latitude"), latitude, "{case}: {leaves:?}");
                    assert_eq!(center("Longitude"), longitude, "{case}: {leaves:?}");
                    assert_eq!(
                        number(&format!("{circle}/Radius")),
                        stretched_number,
                        "{case}"
                    );
                    assert_eq!(
                        number("Restrictions/NumberOfResults"),
                        limit_number,
                        "{case}"
                    );
                    let texts = |name: &str| -> Vec<&str> {
                        let named = leaves.iter().filter(|(path, _)| path.contains(name));
                        named.map(|(_, text)| text.as_str()).collect()
                    };
                    assert_eq!(texts("Restrictions/Type"), ["stop"], "{case}");
                    assert_eq!(texts("LocationName"), Vec::<&str>::new(), "{case}");
                }
                Protocol::HafasMgate => {
                    assert_eq!(request.method, Method::Post, "{case}");
                    assert!(request.url.starts_with(ENDPOINT), "{}", request.url);
                    let mut expected = json!({
                        "ring": {"cCrd": {"x": 13_732_500, "y": 51_040_000}, "maxDist": radius},
                        "getStops": true,
                        "getPOIs": false,
                    });
                    if let Some(limit) = limit {
                        expected["maxLoc"] = json!(limit);
                    }
                    let call = body(&request)["svcReqL"][0].clone();
                    assert_eq!(call["meth"], "LocGeoPos", "{case}");
                    assert_eq!(call["req"], expected, "{case}");
                }
                _ => {
                    assert_eq!(request.method, Method::Post, "{case}");
                    assert_eq!(request.url, ENDPOINT, "the endpoint, not a geocoder");
                    let argument = |name| nearest_argument(&request, name);
                    assert_eq!(argument("latitude"), latitude, "{case}");
                    assert_eq!(argument("longitude"), longitude, "{case}");
                    assert_eq!(argument("maximumDistance"), radius_number, "{case}");
                    assert_eq!(argument("maximumResults"), limit_number, "{case}");
                    let text = body(&request).to_string();
                    assert!(text.contains("filterByPlaceTypes"), "{text}");
                    assert!(text.contains("stopPlace"), "{text}");
                }
            }
        }
    }
}

#[test]
fn position_off_the_globe_is_an_invalid_query_and_a_network_can_lack_the_operation() {
    for protocol in PROTOCOLS {
        for (latitude, longitude) in [
            (91.0, 13.7),
            (51.0, -180.5),
            (f64::NAN, 13.7),
            (51.0, f64::INFINITY),
        ] {
            let query = NearbyQuery::new(Coordinate {
                latitude,
                longitude,
            });
            let result = network(protocol).nearby_request(&query);
            assert!(
                matches!(result, Err(Error::InvalidQuery { .. })),
                "{protocol:?}, {query:?}: {result:?}"
            );
        }

        let mut without = network(protocol);
        without.unsupported.push(Operation::Nearby);
        let result = without.nearby_request(&NearbyQuery::new(DRESDEN));
        assert!(
            matches!(result, Err(Error::Unsupported { .. })),
            "{protocol:?}: {result:?}"
        );
    }
}

#[test]
fn stops_come_in_the_servers_order_near_the_position() {
    type Case<'a> = (Protocol, &'a [u8], (f64, f64), &'a [&'a str], &'a str);
    let dresden = (DRESDEN.latitude, DRESDEN.longitude);
    let dresden_stops = ["de:14612:28", "de:14612:36:0", "de:14612:36", "de:14612:32"];
    let cases: [Case; 6] = [
        (
            Protocol::Efa,
            include_bytes!("fixtures/efa/vvo-nearby.json"),
            dresden,
            &dresden_stops,
            "Hauptbahnhof",
        ),
        (
            Protocol::Motis,
            include_bytes!("fixtures/motis/transitous-nearby.json"),
            dresden,
            &[
                "at-Railway-Current-Reference-Data-2026_de:14612:28:10:Gleis3",
                "de-DELFI_de:14612:32:1:1",
                "de-DELFI_de:14612:29:1:1",
                "de-DELFI_de:14612:34:1:1",
            ],
            "Dresden Hbf",
        ),
        (
            Protocol::Trias,
            include_bytes!("fixtures/trias/vvo-nearby.xml"),
            dresden,
            &dresden_stops,
            "Hauptbahnhof",
        ),
        (
            Protocol::Ojp1,
            include_bytes!("fixtures/ojp/vrr-nearby.xml"),
            (51.2198, 6.7943),
            &["de:05111:18235"],
            "Düsseldorf Hbf",
        ),
        (
            Protocol::HafasMgate,
            include_bytes!("fixtures/hafas/vbb-nearby.json"),
            (52.521_918, 13.413_215),
            &[
                "900100005",
                "900100024",
                "900100003",
                "900100707",
                "900100026",
            ],
            "U Alexanderplatz (Berlin) [Tram]",
        ),
        (
            Protocol::OtpTransmodel,
            include_bytes!("fixtures/otp/entur-nearby.json"),
            (59.9109, 10.7530),
            &["NSR:StopPlace:59872", "NSR:StopPlace:58366"],
            "Oslo S",
        ),
    ];
    for (protocol, fixture, (latitude, longitude), ids, name) in cases {
        let stops = network(protocol)
            .parse_nearby(&respond(200, fixture))
            .unwrap();
        let found: Vec<&str> = stops.iter().map(|stop| stop.id.as_str()).collect();
        assert_eq!(found, ids, "{protocol:?}");
        assert_eq!(stops[0].name, name, "{protocol:?}");
        let asked = Coordinate {
            latitude,
            longitude,
        };
        for stop in &stops {
            assert_eq!(stop.kind, LocationKind::Stop, "{stop:?}");
            let distance = stop.coordinate.map(|at| asked.distance_meters(at));
            assert!(distance.is_some_and(|meters| meters < 1000.0), "{stop:?}");
        }
    }
}

/// The fixtures of the search by name hold one of the stops nearby each.
#[test]
fn stop_is_the_location_the_search_by_name_returns_for_it() {
    let by_name: [(Protocol, &[u8], &[u8], &str); 4] = [
        (
            Protocol::Efa,
            include_bytes!("fixtures/efa/vvo-nearby.json"),
            include_bytes!("fixtures/efa/vvo-stopfinder.json"),
            "de:14612:28",
        ),
        (
            Protocol::Trias,
            include_bytes!("fixtures/trias/vvo-nearby.xml"),
            include_bytes!("fixtures/trias/vvo-locations.xml"),
            "de:14612:28",
        ),
        (
            Protocol::Ojp1,
            include_bytes!("fixtures/ojp/vrr-nearby.xml"),
            include_bytes!("fixtures/ojp/vrr-locations.xml"),
            "de:05111:18235",
        ),
        (
            Protocol::HafasMgate,
            include_bytes!("fixtures/hafas/vbb-nearby.json"),
            include_bytes!("fixtures/hafas/vbb-locations.json"),
            "900100003",
        ),
    ];
    for (protocol, nearby, search, id) in by_name {
        let network = network(protocol);
        let pick = |locations: Vec<Location>| {
            let stop = locations.into_iter().find(|location| location.id == id);
            stop.map(|stop| format!("{stop:?}"))
        };
        let near = pick(network.parse_nearby(&respond(200, nearby)).unwrap());
        let named = pick(network.parse_locations(&respond(200, search)).unwrap());
        assert!(near.is_some(), "{protocol:?}");
        assert_eq!(near, named, "{protocol:?}");
    }
}

#[test]
fn answer_without_stops_is_empty_and_a_failed_one_is_an_error() {
    // What VVO and VBB answered for a circle without stops.
    let empty: [(Protocol, &[u8]); 5] = [
        (Protocol::Efa, br#"{"version":"11.0.6.72","locations":[]}"#),
        (
            Protocol::Trias,
            include_bytes!("fixtures/trias/vvo-nearby-empty.xml"),
        ),
        (
            Protocol::HafasMgate,
            br#"{"ver":"1.33","err":"OK","svcResL":[{"meth":"LocGeoPos","err":"OK","res":{"common":{}}}]}"#,
        ),
        (Protocol::Motis, b"[]"),
        (
            Protocol::OtpTransmodel,
            br#"{"data":{"nearest":{"edges":[]}}}"#,
        ),
    ];
    for (protocol, body) in empty {
        let stops = network(protocol).parse_nearby(&respond(200, body));
        assert_eq!(stops.map(|stops| stops.len()), Ok(0), "{protocol:?}");
    }
    // An error next to an empty list is why the list is empty.
    let failed = br#"{"errors":[{"message":"timeout"}],"data":{"nearest":{"edges":[]}}}"#;
    let stops = network(Protocol::OtpTransmodel).parse_nearby(&respond(200, failed));
    assert!(matches!(stops, Err(Error::Server { .. })), "{stops:?}");

    for protocol in PROTOCOLS {
        let parse = |status, body| network(protocol).parse_nearby(&respond(status, body));
        assert_eq!(parse(503, b""), Err(Error::Unavailable), "{protocol:?}");
        let foreign = parse(200, b"<html>Maintenance</html>");
        assert!(
            matches!(foreign, Err(Error::Parse { .. })),
            "{protocol:?}: {foreign:?}"
        );
    }
}

fn coordinate() -> impl Strategy<Value = Coordinate> {
    (-90.0..=90.0, -180.0..=180.0).prop_map(|(latitude, longitude)| Coordinate {
        latitude,
        longitude,
    })
}

proptest! {
    #[test]
    fn distance_is_symmetric_zero_to_itself_and_at_most_half_way_around(
        a in coordinate(),
        b in coordinate(),
    ) {
        let distance = a.distance_meters(b);
        prop_assert!((distance - b.distance_meters(a)).abs() < 1e-6, "{distance}");
        prop_assert!((0.0..=20_040_000.0).contains(&distance), "{distance}");
        prop_assert!(a.distance_meters(a).abs() < 1e-6);
    }
}

/// A degree of longitude is as long as one of latitude at the equator and
/// half as long at 60° north.
#[test]
fn distance_of_one_degree_follows_the_great_circle() {
    let at = |latitude, longitude| Coordinate {
        latitude,
        longitude,
    };
    for (from, to, meters) in [
        (at(0.0, 0.0), at(1.0, 0.0), 111_195.0),
        (at(0.0, 0.0), at(0.0, 1.0), 111_195.0),
        (at(60.0, 0.0), at(60.0, 1.0), 55_597.0),
    ] {
        let distance = from.distance_meters(to);
        assert!(
            (distance - meters).abs() < 200.0,
            "{from:?} to {to:?}: {distance}"
        );
    }
    // A position that is none has no distance, and not a large one.
    assert!(at(f64::NAN, 0.0).distance_meters(at(0.0, 0.0)).is_nan());
}
