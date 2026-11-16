//! Location search by name, for every protocol.

use proptest::prelude::*;
use publictransport::{
    Error, Header, HttpRequest, HttpResponse, Location, LocationKind, LocationQuery, Method,
    Network, Protocol,
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
    network(protocol, "https://example.org/base").parse_locations(&HttpResponse {
        status,
        headers: Vec::new(),
        body: body.to_vec(),
    })
}

type Fields<'a> = (
    &'a str,
    Option<&'a str>,
    LocationKind,
    &'a str,
    Option<&'a str>,
    Option<(f64, f64)>,
);

/// Id, global id, kind, name, locality and coordinate of a location.
fn fields(location: &Location) -> Fields<'_> {
    (
        &location.id,
        location.global_id.as_deref(),
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

proptest! {
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
fn efa_request_asks_for_rapid_json_in_wgs84_and_passes_the_limit() {
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
    assert_eq!(parameter(&request, "anyMaxSizeHitList"), ["7"]);

    query.limit = None;
    let request = network.locations_request(&query).unwrap();
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
            Some("de:14612:28"),
            LocationKind::Stop,
            "Hauptbahnhof",
            Some("Dresden"),
            Some((51.03993, 13.732_932)),
        )
    );
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
    assert_eq!(poi.global_id, None);
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
    assert_eq!(locations[0].global_id, None);
    assert_eq!(
        fields(&locations[4]),
        (
            "placeID:14612000:1",
            None,
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
            Some("de:14612:28"),
            LocationKind::Stop,
            "Dresden Hauptbahnhof",
            Some("Dresden"),
            Some((51.039_930_000_000_005, 13.732_932)),
        )
    );
    assert_eq!(
        locations[1].global_id.as_deref(),
        Some("de:14612:32:1:1"),
        "a platform"
    );
    let place = &locations[2];
    assert_eq!(place.kind, LocationKind::Poi);
    assert_eq!(place.id, "node/2675542513");
    assert_eq!(place.global_id, None);
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
    assert_eq!(locations[0].global_id, None);
    assert_eq!(locations[1].kind, LocationKind::Poi);
    assert_eq!(locations[1].locality, None);
    assert_eq!(locations[2].global_id, None, "not an IFOPT identifier");
}

#[test]
fn failure_status_wins_over_a_valid_body() {
    for (protocol, body) in PROTOCOLS {
        let status = |status| parse(protocol, status, body).unwrap_err();
        assert_eq!(status(401), Error::Unauthorized);
        assert_eq!(status(403), Error::Blocked);
        assert_eq!(status(429), Error::RateLimited);
        assert_eq!(status(500), Error::Unavailable);
        assert_eq!(status(503), Error::Unavailable);
        assert!(matches!(status(400), Error::Server { status: 400, .. }));
        assert!(matches!(status(404), Error::Server { status: 404, .. }));
        assert_eq!(status(302), Error::Moved { location: None });
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
            network(protocol, "https://example.org/base").parse_locations(&response),
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
