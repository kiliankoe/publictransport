//! Departures at a stop, for every protocol.

use proptest::prelude::*;
use publictransport::{
    Departure, DeparturesQuery, Error, HttpRequest, HttpResponse, LocationKind, Method, Mode,
    Network, Protocol, Timestamp,
};

const VVO: &[u8] = include_bytes!("fixtures/efa/vvo-departures.json");
const UNKNOWN_STOP: &[u8] = include_bytes!("fixtures/efa/vvo-departures-unknown-stop.json");
const VRR: &[u8] = include_bytes!("fixtures/efa/vrr-departures.json");
const TRANSITOUS: &[u8] = include_bytes!("fixtures/motis/transitous-stoptimes.json");
const TRANSITOUS_LOCATIONS: &[u8] = include_bytes!("fixtures/motis/transitous-geocode.json");

const PROTOCOLS: [Protocol; 2] = [Protocol::Efa, Protocol::Motis];

fn network(protocol: Protocol) -> Network {
    Network::new("test", "Test", "https://example.org/base", protocol)
}

fn parse(protocol: Protocol, body: &[u8]) -> Result<Vec<Departure>, Error> {
    network(protocol).parse_departures(&HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    })
}

/// A time of day on 2026-11-16 in UTC, the day the fixtures were recorded.
fn utc(hour: i64, minute: i64, second: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_794_787_200 + hour * 3600 + minute * 60 + second,
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

proptest! {
    #[test]
    fn parsing_never_panics(
        status in 200_u16..300,
        body in prop_oneof![
            any::<Vec<u8>>(),
            (0..VRR.len()).prop_map(|end| VRR[..end].to_vec()),
            (0..TRANSITOUS.len()).prop_map(|end| TRANSITOUS[..end].to_vec()),
        ],
    ) {
        for protocol in PROTOCOLS {
            let _ = network(protocol).parse_departures(&HttpResponse {
                status,
                headers: Vec::new(),
                body: body.clone(),
            });
        }
    }
}

#[test]
fn stop_id_reaches_the_server_under_the_protocols_path() {
    let stop_id = "de:14612:28 &=#/ä";
    for (protocol, prefix, name) in [
        (
            Protocol::Efa,
            "https://example.org/base/XML_DM_REQUEST?",
            "name_dm",
        ),
        (
            Protocol::Motis,
            "https://example.org/base/v6/stoptimes?",
            "stopId",
        ),
    ] {
        let request = network(protocol)
            .departures_request(&DeparturesQuery::new(stop_id))
            .unwrap();
        assert_eq!(request.method, Method::Get);
        assert!(request.url.starts_with(prefix), "{}", request.url);
        assert_eq!(parameter(&request, name), [stop_id]);
    }
}

#[test]
fn time_outside_the_calendar_is_an_invalid_query() {
    let mut query = DeparturesQuery::new("de:14612:28");
    for (protocol, unix_seconds) in [
        (Protocol::Efa, i64::MAX),
        (Protocol::Efa, i64::MIN),
        // About the year 250 BC, which the date parameter cannot express.
        (Protocol::Efa, -70_000_000_000),
        (Protocol::Motis, i64::MAX),
        (Protocol::Motis, i64::MIN),
    ] {
        query.time = Some(Timestamp { unix_seconds });
        let result = network(protocol)
            .with_timezone("Europe/Berlin")
            .departures_request(&query);
        assert!(
            matches!(result, Err(Error::InvalidQuery { .. })),
            "{result:?}"
        );
    }
}

#[test]
fn efa_request_asks_for_realtime_departures_and_omits_what_the_query_leaves_open() {
    let network = network(Protocol::Efa);
    let mut query = DeparturesQuery::new("de:14612:28");
    query.limit = Some(7);
    let request = network.departures_request(&query).unwrap();
    for (name, value) in [
        ("outputFormat", "rapidJSON"),
        ("coordOutputFormat", "WGS84[dd.ddddd]"),
        ("type_dm", "any"),
        ("mode", "direct"),
        ("useRealtime", "1"),
        ("limit", "7"),
    ] {
        assert_eq!(parameter(&request, name), [value], "{name}");
    }

    query.limit = None;
    let request = network.departures_request(&query).unwrap();
    for name in ["limit", "itdDate", "itdTime", "itdDateTimeDepArr"] {
        assert_eq!(parameter(&request, name), Vec::<String>::new(), "{name}");
    }
}

#[test]
fn efa_request_sends_the_wall_clock_time_of_the_network() {
    let network = network(Protocol::Efa).with_timezone("Europe/Berlin");
    let mut query = DeparturesQuery::new("de:14612:28");
    let at = |unix_seconds| Timestamp { unix_seconds };
    for (time, date, time_of_day) in [
        (utc(13, 24, 59), "20261116", "1424"),
        (utc(23, 30, 0), "20261117", "0030"),
        // The hour that repeats when daylight saving time ends has one
        // wall-clock time for two instants.
        (at(1_792_888_200), "20261025", "0230"),
        (at(1_792_891_800), "20261025", "0230"),
        // 2026-07-01T12:00:00Z, during daylight saving time.
        (at(1_782_907_200), "20260701", "1400"),
    ] {
        query.time = Some(time);
        let request = network.departures_request(&query).unwrap();
        assert_eq!(parameter(&request, "itdDate"), [date]);
        assert_eq!(parameter(&request, "itdTime"), [time_of_day]);
        assert_eq!(parameter(&request, "itdDateTimeDepArr"), ["dep"]);
    }
}

#[test]
fn efa_request_with_a_time_needs_a_known_time_zone() {
    let mut query = DeparturesQuery::new("de:14612:28");
    query.time = Some(utc(13, 24, 0));
    for network in [
        network(Protocol::Efa),
        network(Protocol::Efa).with_timezone("Europe/Atlantis"),
    ] {
        let result = network.departures_request(&query);
        assert!(
            matches!(result, Err(Error::InvalidNetwork { .. })),
            "{result:?}"
        );
    }
}

#[test]
fn motis_request_always_sends_a_number_and_the_time_in_utc_without_a_time_zone() {
    let network = network(Protocol::Motis);
    let query = DeparturesQuery::new("de-DELFI_de:14612:28");
    let request = network.departures_request(&query).unwrap();
    assert_eq!(parameter(&request, "n"), ["10"]);
    assert_eq!(parameter(&request, "time"), Vec::<String>::new());

    let query = query.with_limit(7).with_time(utc(13, 24, 0));
    let request = network.departures_request(&query).unwrap();
    assert_eq!(parameter(&request, "n"), ["7"]);
    assert_eq!(parameter(&request, "time"), ["2026-11-16T13:24:00Z"]);
}

#[test]
fn efa_names_a_platform_after_its_station() {
    let response = HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: VVO.to_vec(),
    };
    let departures = network(Protocol::Efa)
        .with_timezone("Europe/Berlin")
        .parse_departures(&response)
        .unwrap();
    assert_eq!(departures.len(), 10);
    // EFA sends no time zone, so the stop gets the one of the network.
    assert_eq!(
        departures[0].stop.timezone.as_deref(),
        Some("Europe/Berlin")
    );
    // The server orders by scheduled time; line 8 is five minutes late.
    assert!(departures.is_sorted_by_key(Departure::expected_time));

    let bus = departures.iter().find(|d| d.line.name == "360").unwrap();
    assert_eq!(bus.stop.id, "de:14612:28:2:6");
    assert_eq!(bus.stop.global_id.as_deref(), Some("de:14612:28:2:6"));
    assert_eq!(bus.stop.kind, LocationKind::Stop);
    assert_eq!(bus.stop.name, "Hauptbahnhof");
    assert_eq!(bus.stop.locality.as_deref(), Some("Dresden"));
    assert_eq!(
        bus.stop.coordinate.map(|c| (c.latitude, c.longitude)),
        Some((51.039_727, 13.733_767))
    );
    assert_eq!(bus.line.name, "360");
    assert_eq!(bus.line.mode, Mode::Bus);
    assert_eq!(bus.line.product.as_deref(), Some("PlusBus"));
    assert_eq!(bus.line.operator.as_deref(), Some("RVSOE"));
    assert_eq!(bus.direction.as_deref(), Some("Possendorf"));
    assert_eq!(bus.scheduled_time, utc(13, 24, 0));
    assert_eq!(bus.realtime_time, Some(utc(13, 25, 0)));
    assert_eq!(bus.scheduled_platform.as_deref(), Some("6"));
    assert_eq!(bus.realtime_platform.as_deref(), Some("6"));
    assert!(!bus.cancelled);

    let tram = departures
        .iter()
        .find(|d| d.direction.as_deref() == Some("MESSE DRESDEN"))
        .unwrap();
    assert_eq!(tram.line.mode, Mode::Tram);
    assert_eq!(tram.scheduled_time, utc(13, 25, 0));
    assert_eq!(tram.realtime_time, Some(utc(13, 26, 48)));
}

#[test]
fn efa_tells_the_planned_platform_from_the_current_one() {
    let departures = parse(Protocol::Efa, VRR).unwrap();
    let fields = |name: &str| {
        let departure = departures.iter().find(|d| d.line.name == name).unwrap();
        (
            departure.line.mode,
            departure.scheduled_platform.as_deref(),
            departure.realtime_platform.as_deref(),
        )
    };
    assert_eq!(fields("RE13").0, Mode::RegionalTrain, "class 13");
    assert_eq!(fields("U79"), (Mode::Subway, None, Some("2")));
    assert_eq!(
        fields("S 11"),
        (Mode::SuburbanTrain, Some("13"), Some("14"))
    );
    // A train without a line is named by its category and number.
    assert_eq!(fields("ICE 109").0, Mode::HighSpeedTrain, "class 16");
    let stop = &departures[0].stop;
    assert_eq!(stop.timezone, None, "the network has no time zone");
    assert_eq!(stop.name, "Düsseldorf Hbf");
    assert_eq!(stop.locality.as_deref(), Some("Düsseldorf"));
}

/// VVO files every train under class 0. These two are from a live response.
#[test]
fn efa_tells_long_distance_trains_in_the_generic_train_class_by_their_category() {
    let event = |number: &str, train_type: &str, line_display: &str| {
        format!(
            r#"{{"location":{{"id":"de:14612:28","isGlobalId":true,"name":"Dresden Hauptbahnhof","type":"stop"}},"departureTimePlanned":"2026-11-17T07:00:00Z","transportation":{{"number":"{number}","product":{{"class":0,"name":"Zug"}},"properties":{{"trainType":"{train_type}","lineDisplay":"{line_display}"}}}}}}"#
        )
    };
    let body = format!(
        r#"{{"version":"11.0.6.72","stopEvents":[{},{},{}]}}"#,
        event("2608", "IC", "TRAIN"),
        event("171", "RJ", "TRAIN"),
        event("RE50", "NRE", "LINE"),
    );
    let departures = parse(Protocol::Efa, body.as_bytes()).unwrap();
    let lines: Vec<_> = departures
        .iter()
        .map(|d| (d.line.name.as_str(), d.line.mode))
        .collect();
    assert_eq!(
        lines,
        [
            ("IC 2608", Mode::LongDistanceTrain),
            ("RJ 171", Mode::HighSpeedTrain),
            ("RE50", Mode::RegionalTrain),
        ]
    );
}

#[test]
fn efa_reads_cancellations_and_tolerates_missing_optional_fields() {
    let event = |minute: u8, extra: &str| {
        format!(
            r#"{{"location":{{"id":"33000028","isGlobalId":false,"name":"Dresden Hauptbahnhof","disassembledName":"Hauptbahnhof","type":"stop","coord":[51.03993,13.732932],"parent":{{"name":"Dresden","type":"locality"}}}},"departureTimePlanned":"2026-11-16T13:{minute}:00Z","transportation":{{"id":"x","name":"Sonderzug 1","number":"1","product":{{"id":99,"class":99,"name":"Sonderzug"}},"destination":{{"id":"33000001","name":"Pirna","type":"stop"}}}}{extra}}}"#
        )
    };
    let body = format!(
        r#"{{"version":"11.0.6.72","stopEvents":[{},{},{}]}}"#,
        event(24, r#","isCancelled":true"#),
        event(25, r#","realtimeStatus":["MONITORED","TRIP_CANCELLED"]"#),
        event(26, r#","realtimeStatus":["MONITORED"]"#),
    );
    let departures = parse(Protocol::Efa, body.as_bytes()).unwrap();
    let cancelled: Vec<_> = departures.iter().map(|d| d.cancelled).collect();
    assert_eq!(cancelled, [true, true, false]);

    let departure = &departures[2];
    assert_eq!(departure.stop.id, "33000028");
    assert_eq!(departure.stop.global_id, None);
    assert_eq!(departure.stop.station_id(), None);
    assert_eq!(departure.stop.kind, LocationKind::Stop);
    assert_eq!(departure.line.mode, Mode::Other);
    assert_eq!(departure.line.operator, None);
    assert_eq!(departure.realtime_time, None);
    assert_eq!(departure.scheduled_platform, None);
    assert_eq!(departure.realtime_platform, None);
}

#[test]
fn motis_orders_by_expected_time_and_reads_the_stop_from_the_place() {
    let departures = parse(Protocol::Motis, TRANSITOUS).unwrap();
    assert_eq!(departures.len(), 10);
    assert!(departures.is_sorted_by_key(Departure::expected_time));
    assert_eq!(departures[0].direction.as_deref(), Some("MESSE DRESDEN"));

    let tram = departures
        .iter()
        .find(|d| d.direction.as_deref() == Some("MESSE DRESDEN"))
        .unwrap();
    assert_eq!(tram.stop.id, "de-DELFI_de:14612:28:3:2");
    assert_eq!(tram.stop.global_id.as_deref(), Some("de:14612:28:3:2"));
    assert_eq!(tram.stop.kind, LocationKind::Stop);
    assert_eq!(tram.stop.name, "Dresden Hauptbahnhof");
    assert_eq!(tram.stop.locality, None);
    assert_eq!(tram.stop.timezone.as_deref(), Some("Europe/Berlin"));
    // The JSON parser may be off by one unit in the last place.
    let coordinate = tram.stop.coordinate.unwrap();
    assert!(
        (coordinate.latitude - 51.041_096).abs() < 1e-9,
        "{coordinate:?}"
    );
    assert!(
        (coordinate.longitude - 13.732_689).abs() < 1e-9,
        "{coordinate:?}"
    );
    assert_eq!(tram.line.name, "10");
    assert_eq!(tram.line.mode, Mode::Tram);
    assert_eq!(tram.line.product, None);
    assert_eq!(tram.line.operator.as_deref(), Some("DVB-Straßenbahn"));
    assert_eq!(tram.scheduled_time, utc(13, 28, 0));
    assert_eq!(tram.realtime_time, Some(utc(13, 25, 0)));
    assert_eq!(tram.scheduled_platform.as_deref(), Some("2"));
    assert_eq!(tram.realtime_platform.as_deref(), Some("2"));
    assert!(!tram.cancelled);
}

#[test]
fn motis_reads_cancellations_modes_and_departures_without_realtime() {
    let stop_time = |minute: u8, mode: &str, real_time: bool, cancelled: [bool; 3]| {
        let [departure, trip, place] = cancelled;
        format!(
            r#"{{"place":{{"name":"Wien Hauptbahnhof","stopId":"at-Railway-Current-Reference-Data-2026_Pat:49:1349","lat":48.18,"lon":16.37,"departure":"2026-11-16T13:{minute}:00Z","scheduledDeparture":"2026-11-16T13:{minute}:00Z","cancelled":{place}}},"mode":"{mode}","realTime":{real_time},"headsign":"Graz","agencyName":"ÖBB","routeShortName":"RJ 1","cancelled":{departure},"tripCancelled":{trip}}}"#
        )
    };
    let body = format!(
        r#"{{"stopTimes":[{},{},{},{},{}],"previousPageCursor":"a","nextPageCursor":"b"}}"#,
        stop_time(24, "HIGHSPEED_RAIL", false, [false, false, false]),
        stop_time(25, "REGIONAL_FAST_RAIL", true, [true, false, false]),
        stop_time(26, "AERIAL_LIFT", true, [false, true, false]),
        stop_time(27, "ODM", true, [false, false, true]),
        stop_time(28, "HOVERCRAFT", true, [false, false, false]),
    );
    let departures = parse(Protocol::Motis, body.as_bytes()).unwrap();
    let fields: Vec<_> = departures
        .iter()
        .map(|d| (d.line.mode, d.realtime_time, d.cancelled))
        .collect();
    assert_eq!(
        fields,
        [
            (Mode::HighSpeedTrain, None, false),
            (Mode::RegionalTrain, Some(utc(13, 25, 0)), true),
            (Mode::CableCar, Some(utc(13, 26, 0)), true),
            (Mode::OnDemand, Some(utc(13, 27, 0)), true),
            (Mode::Other, Some(utc(13, 28, 0)), false),
        ]
    );

    let departure = &departures[0];
    assert_eq!(departure.line.name, "RJ 1", "no displayName");
    assert_eq!(departure.stop.global_id, None, "not an IFOPT identifier");
    assert_eq!(departure.scheduled_platform, None);
    assert_eq!(departure.realtime_platform, None);
}

/// Both fixtures contain bus 66 to Nickern, leaving from platform 5.
#[test]
fn both_protocols_describe_the_same_departure_alike() {
    let shared = |protocol, body| {
        let departures = parse(protocol, body).unwrap();
        let bus = departures.iter().find(|d| d.line.name == "66").unwrap();
        (
            bus.scheduled_time,
            bus.line.mode,
            bus.scheduled_platform.clone(),
            bus.realtime_platform.clone(),
            bus.stop.station_id().map(str::to_owned),
        )
    };
    let efa = shared(Protocol::Efa, VVO);
    assert_eq!(efa, shared(Protocol::Motis, TRANSITOUS));
    assert_eq!(
        efa,
        (
            utc(13, 27, 0),
            Mode::Bus,
            Some("5".into()),
            Some("5".into()),
            Some("de:14612:28".into()),
        )
    );
}

#[test]
fn station_id_of_a_station_is_its_global_id() {
    let body = br#"{"version":"11.0.6.72","locations":[{"id":"de:14612:28","isGlobalId":true,"name":"Dresden, Hauptbahnhof","type":"stop"}]}"#;
    let response = HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    };
    let locations = network(Protocol::Efa).parse_locations(&response).unwrap();
    assert_eq!(locations[0].station_id(), Some("de:14612:28"));
}

/// The id of a place without a stop would be of no use in a query.
#[test]
fn motis_stop_time_without_a_stop_is_a_parse_error() {
    let body = br#"{"stopTimes":[{"place":{"name":"X","lat":51.04,"lon":13.73,"scheduledDeparture":"2026-11-16T13:24:00Z"},"mode":"TRAM"}]}"#;
    let result = parse(Protocol::Motis, body);
    assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
}

#[test]
fn response_without_departures_is_an_empty_list() {
    for (protocol, body) in [
        (
            Protocol::Efa,
            br#"{"version":"11.0.6.72","systemMessages":[],"locations":[{"id":"de:14612:28","isGlobalId":true,"name":"Dresden, Hauptbahnhof","type":"stop","isBest":true}]}"#.as_slice(),
        ),
        (
            Protocol::Motis,
            br#"{"stopTimes":[],"previousPageCursor":"a","nextPageCursor":"b"}"#,
        ),
    ] {
        assert_eq!(parse(protocol, body), Ok(Vec::new()));
    }
}

/// EFA answers with status 200 and the stops it considered, MOTIS with
/// status 404 and this body.
#[test]
fn unknown_stop_is_not_found() {
    let result = parse(Protocol::Efa, UNKNOWN_STOP);
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");

    let response = HttpResponse {
        status: 404,
        headers: Vec::new(),
        body: br#"{"error":"no radius: stop_found=false, center_parsed=false"}"#.to_vec(),
    };
    let result = network(Protocol::Motis).parse_departures(&response);
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");
}

#[test]
fn success_status_with_a_foreign_body_is_a_parse_error() {
    for (protocol, other_protocol_body) in [(Protocol::Efa, TRANSITOUS), (Protocol::Motis, VVO)] {
        for body in [
            other_protocol_body,
            TRANSITOUS_LOCATIONS,
            b"<html>Maintenance</html>",
            b"{}",
            b"",
        ] {
            let result = parse(protocol, body);
            assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
        }
    }
}
