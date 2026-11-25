//! Location search by name, for every protocol.

use proptest::prelude::*;
use publictransport::{
    DeparturesQuery, Error, GlobalId, Header, HttpRequest, HttpResponse, IdScheme, Location,
    LocationKind, LocationQuery, Method, Network, Place, Protocol, TripsQuery,
};

const VVO: &[u8] = include_bytes!("fixtures/efa/vvo-stopfinder.json");
const VRR: &[u8] = include_bytes!("fixtures/efa/vrr-stopfinder.json");
const TRANSITOUS: &[u8] = include_bytes!("fixtures/motis/transitous-geocode.json");

/// Each protocol with a response it accepts.
const PROTOCOLS: [(Protocol, &[u8]); 2] = [(Protocol::Efa, VVO), (Protocol::Motis, TRANSITOUS)];

fn network(protocol: Protocol, endpoint: &str) -> Network {
    Network::new("test", "Test", endpoint, protocol)
}

fn parse(protocol: Protocol, status: u16, body: &[u8]) -> Result<Vec<Location>, Error> {
    network(protocol, "https://example.org/base").parse_locations(
        &publictransport::LocationQuery::new("text"),
        &HttpResponse {
            status,
            headers: Vec::new(),
            body: body.to_vec(),
        },
    )
}

fn ifopt(value: &str) -> GlobalId {
    GlobalId {
        scheme: IdScheme::Ifopt,
        value: value.to_owned(),
    }
}

type Fields<'a> = (
    &'a str,
    Vec<GlobalId>,
    LocationKind,
    &'a str,
    Option<&'a str>,
    Option<(f64, f64)>,
);

/// Id, global ids, kind, name, locality and coordinate of a location.
fn fields(location: &Location) -> Fields<'_> {
    (
        &location.id,
        location.global_ids.clone(),
        location.kind,
        &location.name,
        location.locality.as_deref(),
        location.coordinate.map(|c| (c.latitude, c.longitude)),
    )
}

/// The decoded values of every query parameter called `name`.
fn parameter(request: &HttpRequest, name: &str) -> Vec<String> {
    let query = request.url.split_once('?').map_or("", |(_, query)| query);
    form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// Any choice of kinds, in any order and with duplicates.
fn kinds() -> impl Strategy<Value = Vec<LocationKind>> {
    prop::collection::vec(
        prop::sample::select(vec![
            LocationKind::Stop,
            LocationKind::Address,
            LocationKind::Poi,
            LocationKind::Other,
        ]),
        0..8,
    )
}

proptest! {
    #[test]
    fn efa_filter_is_the_sum_of_the_bits_of_the_wanted_kinds(kinds in kinds()) {
        let bits = |kind| match kind {
            LocationKind::Stop => 2_u32,
            LocationKind::Address => 12,
            LocationKind::Poi => 32,
            // `Other`; the enum is non-exhaustive.
            _ => 81,
        };
        let sum = kinds.iter().fold(0, |sum, &kind| sum | bits(kind));
        let expected = if sum == 0 { Vec::new() } else { vec![sum.to_string()] };
        let query = LocationQuery::new("Hauptbahnhof").with_kinds(kinds);
        let request = network(Protocol::Efa, "https://example.org/base")
            .locations_request(&query)
            .unwrap();
        prop_assert_eq!(parameter(&request, "anyObjFilter_sf"), expected);
    }

    #[test]
    fn motis_type_lists_each_wanted_kind_once_and_rejects_a_query_for_other_kinds_only(
        kinds in kinds(),
    ) {
        let expected: std::collections::BTreeSet<_> = kinds
            .iter()
            .filter_map(|kind| match kind {
                LocationKind::Stop => Some("STOP"),
                LocationKind::Address => Some("ADDRESS"),
                LocationKind::Poi => Some("PLACE"),
                _ => None,
            })
            .collect();
        let query = LocationQuery::new("Hauptbahnhof").with_kinds(kinds.clone());
        let result = network(Protocol::Motis, "https://example.org/base").locations_request(&query);
        if expected.is_empty() && !kinds.is_empty() {
            prop_assert!(matches!(result, Err(Error::InvalidQuery { .. })), "{:?}", result);
        } else {
            let lists = parameter(&result.unwrap(), "type");
            prop_assert_eq!(lists.len(), usize::from(!kinds.is_empty()), "{:?}", lists);
            let mut values: Vec<_> = lists.iter().flat_map(|list| list.split(',')).collect();
            values.sort_unstable();
            prop_assert_eq!(values, expected.into_iter().collect::<Vec<_>>());
        }
    }

    #[test]
    fn search_text_reaches_the_server_intact(text: String, trailing_slash: bool) {
        let endpoint = if trailing_slash { "https://example.org/base/" } else { "https://example.org/base" };
        let query = LocationQuery::new(text.clone());
        for (protocol, prefix, name) in [
            (Protocol::Efa, "https://example.org/base/XML_STOPFINDER_REQUEST?", "name_sf"),
            (Protocol::Motis, "https://example.org/base/v1/geocode?", "text"),
        ] {
            let request = network(protocol, endpoint).locations_request(&query).unwrap();
            prop_assert_eq!(request.method, Method::Get);
            prop_assert_eq!(&request.body, &None);
            prop_assert!(request.url.bytes().all(|byte| byte.is_ascii_graphic()), "{}", request.url);
            prop_assert!(request.url.starts_with(prefix), "{}", request.url);
            prop_assert_eq!(parameter(&request, name), [text.clone()]);
        }
    }

    #[test]
    fn parsing_never_panics(
        status in 200_u16..300,
        body in prop_oneof![
            any::<Vec<u8>>(),
            (0..VRR.len()).prop_map(|end| VRR[..end].to_vec()),
            (0..TRANSITOUS.len()).prop_map(|end| TRANSITOUS[..end].to_vec()),
        ],
    ) {
        for (protocol, _) in PROTOCOLS {
            let _ = parse(protocol, status, &body);
        }
    }
}

#[test]
fn motis_request_passes_the_limit() {
    let network = network(Protocol::Motis, "https://api.transitous.org/api");
    let mut query = LocationQuery::new("Hauptbahnhof").with_limit(7);
    let request = network.locations_request(&query).unwrap();
    assert_eq!(parameter(&request, "numResults"), ["7"]);

    query.limit = None;
    let request = network.locations_request(&query).unwrap();
    assert_eq!(parameter(&request, "numResults"), Vec::<String>::new());
}

#[test]
fn efa_request_asks_for_rapid_json_in_wgs84_and_sends_no_limit() {
    let network = network(Protocol::Efa, "https://efa.vvo-online.de/std3");
    let mut query = LocationQuery::new("Hauptbahnhof");
    query.limit = Some(7);
    let request = network.locations_request(&query).unwrap();
    assert_eq!(parameter(&request, "outputFormat"), ["rapidJSON"]);
    assert_eq!(
        parameter(&request, "coordOutputFormat"),
        ["WGS84[dd.ddddd]"]
    );
    assert_eq!(parameter(&request, "type_sf"), ["any"]);
    // The server's limit makes vague searches come back empty.
    assert_eq!(
        parameter(&request, "anyMaxSizeHitList"),
        Vec::<String>::new()
    );
}

#[test]
fn efa_splits_the_locality_from_the_name() {
    let locations = parse(Protocol::Efa, 200, VVO).unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(
        fields(&locations[0]),
        (
            "de:14612:28",
            vec![ifopt("de:14612:28")],
            LocationKind::Stop,
            "Hauptbahnhof",
            Some("Dresden"),
            Some((51.03993, 13.732_932)),
        )
    );
}

/// Bavaria's server sends a locality without a name for some stops, and
/// neither MOTIS server of a single operator names a time zone.
#[test]
fn a_location_has_no_empty_locality_and_falls_back_to_the_time_zone_of_the_network() {
    let efa = br#"{"version":"11.0.6.72","locations":[{"id":"de:09162:6","isGlobalId":true,"name":"M\u00fcnchen Hbf","type":"stop","parent":{"name":"","type":"locality"}}]}"#;
    let motis = br#"[{"type":"STOP","name":"Mannheim Hauptbahnhof","id":"rnv_2417","lat":49.48,"lon":8.47,"areas":[],"tz":null}]"#;
    for (protocol, body) in [(Protocol::Efa, &efa[..]), (Protocol::Motis, &motis[..])] {
        let network = network(protocol, "https://example.org/base").with_timezone("Europe/Berlin");
        let locations = network
            .parse_locations(
                &publictransport::LocationQuery::new("text"),
                &HttpResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: body.to_vec(),
                },
            )
            .unwrap();
        assert_eq!(locations[0].locality, None, "{protocol:?}");
        assert_eq!(
            locations[0].timezone.as_deref(),
            Some("Europe/Berlin"),
            "{protocol:?}"
        );
    }
}

/// The VRR response reports an ambiguous search as an error system message.
#[test]
fn efa_ambiguous_search_lists_every_match_best_first() {
    let locations = parse(Protocol::Efa, 200, VRR).unwrap();
    let names: Vec<_> = locations.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Düsseldorf Hbf",
            "Hauptbahnhof Düsseldorf",
            "B&R, Düsseldorf Hauptbahnhof",
            "Radstation Düsseldorf Hauptbahnhof",
        ]
    );
    let poi = &locations[1];
    assert_eq!(poi.kind, LocationKind::Poi);
    assert_eq!(poi.global_ids, []);
    assert!(poi.id.starts_with("poiID:302375:"), "{}", poi.id);
    assert_eq!(poi.locality.as_deref(), Some("Düsseldorf"));
}

#[test]
fn efa_sorts_by_match_quality() {
    let body = br#"{"version":"11.0.6.72","locations":[
        {"id":"a","name":"Worse","type":"stop","matchQuality":100,"isBest":false},
        {"id":"b","name":"Better","type":"stop","matchQuality":1000,"isBest":true}
    ]}"#;
    let locations = parse(Protocol::Efa, 200, body).unwrap();
    let names: Vec<_> = locations.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Better", "Worse"]);
}

#[test]
fn efa_maps_types_to_kinds_and_tolerates_missing_optional_fields() {
    let entry = |kind: &str| {
        format!(
            r#"{{"id":"x:1","isGlobalId":false,"name":"Dresden, Pirnaischer Platz","disassembledName":"Pirnaischer Platz","coord":[51.05,13.74],"type":"{kind}","matchQuality":500,"isBest":false,"parent":{{"id":"placeID:14612000:1","name":"Dresden","type":"locality"}}}}"#
        )
    };
    let body = format!(
        r#"{{"version":"11.0.6.72","locations":[{},{},{},{},{{"id":"placeID:14612000:1","name":"Dresden","type":"locality","matchQuality":400,"isBest":false}}]}}"#,
        entry("street"),
        entry("singlehouse"),
        entry("poi"),
        entry("crossing"),
    );
    let locations = parse(Protocol::Efa, 200, body.as_bytes()).unwrap();
    let kinds: Vec<_> = locations.iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        [
            LocationKind::Address,
            LocationKind::Address,
            LocationKind::Poi,
            LocationKind::Other,
            LocationKind::Other,
        ]
    );
    assert_eq!(locations[0].global_ids, []);
    assert_eq!(
        fields(&locations[4]),
        (
            "placeID:14612000:1",
            Vec::new(),
            LocationKind::Other,
            "Dresden",
            None,
            None
        )
    );
}

#[test]
fn motis_takes_the_locality_from_the_default_area_and_the_global_id_from_the_id() {
    let locations = parse(Protocol::Motis, 200, TRANSITOUS).unwrap();
    assert_eq!(
        fields(&locations[0]),
        (
            "de-DELFI_de:14612:28",
            vec![ifopt("de:14612:28")],
            LocationKind::Stop,
            "Dresden Hauptbahnhof",
            Some("Dresden"),
            Some((51.039_930_000_000_005, 13.732_932)),
        )
    );
    assert_eq!(
        locations[1].global_ids,
        [ifopt("de:14612:32:1:1")],
        "a platform"
    );
    let place = &locations[2];
    assert_eq!(place.kind, LocationKind::Poi);
    assert_eq!(place.id, "node/2675542513");
    assert_eq!(place.global_ids, []);
    assert_eq!(place.timezone.as_deref(), Some("Europe/Berlin"));
}

#[test]
fn motis_maps_types_to_kinds_and_tolerates_a_missing_default_area() {
    let body = br#"[
        {"type":"ADDRESS","tokens":[[0,5]],"name":"Wiener Platz","id":"123456","lat":51.04,"lon":13.73,"street":"Wiener Platz","houseNumber":"1","areas":[{"name":"Dresden","adminLevel":6,"matched":false,"unique":true,"default":true}],"score":-1.5},
        {"type":"PLACE","tokens":[[0,5]],"name":"Kulturpalast","id":"987654","lat":51.05,"lon":13.74,"areas":[{"name":"Sachsen","adminLevel":4,"matched":false,"unique":false,"default":false}],"score":-2.5},
        {"type":"STOP","tokens":[[0,4]],"name":"Wien Hauptbahnhof","id":"at-Railway-Current-Reference-Data-2026_Pat:49:1349","lat":48.18,"lon":16.37,"areas":[],"score":-3.5}
    ]"#;
    let locations = parse(Protocol::Motis, 200, body).unwrap();
    assert_eq!(locations[0].kind, LocationKind::Address);
    assert_eq!(locations[0].global_ids, []);
    assert_eq!(locations[1].kind, LocationKind::Poi);
    assert_eq!(locations[1].locality, None);
    assert_eq!(locations[2].global_ids, [], "not an IFOPT identifier");
}

#[test]
fn every_request_carries_the_headers_of_the_network_after_those_of_the_protocol() {
    let extra = [("X-Api-Key", "secret"), ("X-Client", "tests/1.0")];
    let stop = || Place::Id("de:14612:28".into());
    for protocol in [
        Protocol::Efa,
        Protocol::Motis,
        Protocol::Trias,
        Protocol::Ojp1,
    ] {
        let plain =
            || network(protocol, "https://example.org/base").with_requestor_ref("OpenService");
        let with_headers = extra.iter().fold(plain(), |network, (name, value)| {
            network.with_header(*name, *value)
        });
        let requests = |network: &Network| {
            [
                network.locations_request(&LocationQuery::new("Hauptbahnhof")),
                network.departures_request(&DeparturesQuery::new("de:14612:28")),
                network.trips_request(&TripsQuery::new(stop(), stop())),
            ]
            .map(Result::unwrap)
        };
        for (mut expected, with_headers) in
            requests(&plain()).into_iter().zip(requests(&with_headers))
        {
            expected.headers.extend(extra.map(|(name, value)| Header {
                name: name.to_owned(),
                value: value.to_owned(),
            }));
            assert_eq!(with_headers, expected, "{protocol:?}");
        }
    }
}

/// A key in a header would otherwise end up in the logs of an application.
#[test]
fn debug_output_leaves_header_values_out() {
    let network = network(Protocol::Efa, "https://example.org").with_header("X-Api-Key", "hunter2");
    let request = network
        .locations_request(&LocationQuery::new("Hauptbahnhof"))
        .unwrap();
    for text in [format!("{network:?}"), format!("{request:?}")] {
        assert!(
            text.contains("X-Api-Key") && !text.contains("hunter2"),
            "{text}"
        );
    }
}

/// Two headers of one name would both be sent.
#[test]
fn header_of_the_network_takes_the_place_of_the_one_the_protocol_sets() {
    let request = network(Protocol::Trias, "https://example.org/trias")
        .with_requestor_ref("OpenService")
        .with_header("content-type", "text/xml")
        .locations_request(&LocationQuery::new("Hauptbahnhof"))
        .unwrap();
    let types: Vec<_> = request
        .headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.as_str())
        .collect();
    assert_eq!(types, ["text/xml"]);
}

#[test]
fn failure_status_wins_over_a_valid_body() {
    for (protocol, body) in PROTOCOLS {
        let status = |status| parse(protocol, status, body).unwrap_err();
        assert_eq!(status(401), Error::Unauthorized);
        assert_eq!(status(403), Error::Blocked);
        assert_eq!(
            status(429),
            Error::RateLimited {
                retry_after_seconds: None
            }
        );
        assert_eq!(status(500), Error::Unavailable);
        assert_eq!(status(503), Error::Unavailable);
        assert!(matches!(status(400), Error::Server { status: 400, .. }));
        assert!(matches!(status(404), Error::Server { status: 404, .. }));
        assert_eq!(status(302), Error::Moved { location: None });
    }
    // An application may show the message as it is.
    let moved = |location: Option<&str>| {
        let location = location.map(str::to_owned);
        Error::Moved { location }.to_string()
    };
    assert_eq!(moved(None), "the endpoint has moved");
    assert_eq!(
        moved(Some("https://new.example")),
        "the endpoint has moved to https://new.example"
    );
}

/// The core has no clock, so a date to wait for counts from the `Date` of
/// the same response.
#[test]
fn rate_limit_reports_how_long_the_server_asks_to_wait() {
    const DATE: &str = "Wed, 21 Oct 2015 07:28:00 GMT";
    type Headers<'a> = &'a [(&'a str, &'a str)];
    let cases: [(Headers, Option<u32>); 7] = [
        (&[("Retry-After", "120")], Some(120)),
        (&[("rETRY-aFTER", "120")], Some(120)),
        (
            &[
                ("dATE", DATE),
                ("Retry-After", "Wed, 21 Oct 2015 07:30:00 GMT"),
            ],
            Some(120),
        ),
        (
            &[
                ("Date", DATE),
                ("Retry-After", "Wed, 21 Oct 2015 07:27:00 GMT"),
            ],
            Some(0),
        ),
        (&[("Retry-After", DATE)], None),
        (&[("Date", "today"), ("Retry-After", DATE)], None),
        (&[("Date", DATE), ("Retry-After", "soon")], None),
    ];
    for (headers, retry_after_seconds) in cases {
        let response = HttpResponse {
            status: 429,
            headers: headers
                .iter()
                .map(|&(name, value)| Header {
                    name: name.into(),
                    value: value.into(),
                })
                .collect(),
            body: Vec::new(),
        };
        assert_eq!(
            network(Protocol::Efa, "https://example.org/base")
                .parse_locations(&publictransport::LocationQuery::new("text"), &response)
                .unwrap_err(),
            Error::RateLimited {
                retry_after_seconds
            },
            "{headers:?}"
        );
    }
}

#[test]
fn redirect_reports_the_new_location_whatever_the_header_case() {
    for (protocol, body) in PROTOCOLS {
        let response = HttpResponse {
            status: 301,
            headers: vec![
                Header {
                    name: "content-type".into(),
                    value: "text/html".into(),
                },
                Header {
                    name: "lOCATION".into(),
                    value: "https://example.org/new".into(),
                },
            ],
            body: body.to_vec(),
        };
        assert_eq!(
            network(protocol, "https://example.org/base")
                .parse_locations(&publictransport::LocationQuery::new("text"), &response),
            Err(Error::Moved {
                location: Some("https://example.org/new".into())
            })
        );
    }
}

#[test]
fn success_status_with_a_foreign_body_is_a_parse_error() {
    for (protocol, other_protocol_body) in [(Protocol::Efa, TRANSITOUS), (Protocol::Motis, VVO)] {
        for body in [other_protocol_body, b"<html>Maintenance</html>", b""] {
            let result = parse(protocol, 200, body);
            assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
        }
    }
}
