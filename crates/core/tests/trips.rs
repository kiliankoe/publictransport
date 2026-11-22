//! Trips between two places, for every protocol.

use proptest::prelude::*;
use publictransport::{
    Coordinate, Error, GlobalId, HttpRequest, HttpResponse, IdScheme, Leg, LegEnd, LegKind, Method,
    Mode, Network, Place, Protocol, Stopover, Timestamp, Trip, Trips, TripsQuery,
};

const VVO: &[u8] = include_bytes!("fixtures/efa/vvo-trip.json");
const VRR: &[u8] = include_bytes!("fixtures/efa/vrr-trip.json");
const TRANSITOUS: &[u8] = include_bytes!("fixtures/motis/transitous-plan.json");
const TRIAS: &[u8] = include_bytes!("fixtures/trias/vvo-trip.xml");
const OJP: &[u8] = include_bytes!("fixtures/ojp/vrr-trip.xml");

/// A protocol, a recorded response and the legs of each of its trips.
type Fixture = (Protocol, &'static [u8], &'static [&'static [&'static str]]);

fn network(protocol: Protocol) -> Network {
    Network::new("test", "Test", "https://example.org/base", protocol)
}

/// A network that can build every request: EFA needs the time zone, TRIAS
/// and OJP the requestor reference.
fn complete_network(protocol: Protocol) -> Network {
    network(protocol)
        .with_timezone("Europe/Berlin")
        .with_requestor_ref("OpenService")
}

fn parse(protocol: Protocol, body: &[u8]) -> Result<Trips, Error> {
    complete_network(protocol).parse_trips(&HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    })
}

fn ifopt(value: &str) -> GlobalId {
    GlobalId {
        scheme: IdScheme::Ifopt,
        value: value.to_owned(),
    }
}

/// Every query parameter, decoded.
fn parameters(request: &HttpRequest) -> Vec<(String, String)> {
    let query = request.url.split_once('?').map_or("", |(_, query)| query);
    form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

/// A time of day on 2026-11-16 in UTC, the day the fixtures were recorded.
fn utc(hour: i64, minute: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_794_787_200 + hour * 3600 + minute * 60,
    }
}

/// The decoded values of every query parameter called `name`.
fn parameter(request: &HttpRequest, name: &str) -> Vec<String> {
    parameters(request)
        .into_iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value)
        .collect()
}

/// Whether `text` is `value` as a decimal number, to about a meter.
fn is_degrees(text: &str, value: f64) -> bool {
    text.parse::<f64>()
        .is_ok_and(|number| (number - value).abs() < 1e-5)
}

/// Scheduled and predicted arrival, then scheduled and predicted departure,
/// at a call between the two ends of a leg.
fn times(stopover: &Stopover) -> [Option<Timestamp>; 4] {
    [
        stopover.scheduled_arrival,
        stopover.realtime_arrival,
        stopover.scheduled_departure,
        stopover.realtime_departure,
    ]
}

/// Scheduled and predicted time.
fn end_times(end: &LegEnd) -> (Timestamp, Option<Timestamp>) {
    (end.scheduled_time, end.realtime_time)
}

/// Scheduled and current platform.
fn platforms(end: &LegEnd) -> (Option<&str>, Option<&str>) {
    (
        end.scheduled_platform.as_deref(),
        end.realtime_platform.as_deref(),
    )
}

/// The line name of a ride, or how else the leg is traveled.
fn label(leg: &Leg) -> &str {
    match &leg.kind {
        LegKind::Transit { line, .. } => &line.name,
        LegKind::Walk => "walk",
        _ => "other",
    }
}

fn place() -> impl Strategy<Value = Place> {
    prop_oneof![
        any::<String>().prop_map(Place::Id),
        (-90.0..=90.0, -180.0..=180.0).prop_map(|(latitude, longitude)| {
            Place::Coordinate(Coordinate {
                latitude,
                longitude,
            })
        }),
    ]
}

proptest! {
    #[test]
    fn both_places_reach_the_server_under_the_protocols_path(from in place(), to in place()) {
        let query = TripsQuery::new(from.clone(), to.clone());

        let request = network(Protocol::Efa).trips_request(&query).unwrap();
        prop_assert_eq!(request.method, Method::Get);
        prop_assert!(
            request.url.starts_with("https://example.org/base/XML_TRIP_REQUEST2?"),
            "{}",
            request.url
        );
        for (place, end) in [(&from, "origin"), (&to, "destination")] {
            let kind = parameter(&request, &format!("type_{end}"));
            let name = parameter(&request, &format!("name_{end}"));
            match place {
                Place::Id(id) => {
                    prop_assert_eq!(kind, ["any"]);
                    prop_assert_eq!(name, [id.clone()]);
                }
                Place::Coordinate(coordinate) => {
                    prop_assert_eq!(kind, ["coord"]);
                    let name = name.concat();
                    let parts: Vec<_> = name.split(':').collect();
                    prop_assert_eq!(parts.len(), 3, "{}", name);
                    prop_assert!(is_degrees(parts[0], coordinate.longitude), "{}", name);
                    prop_assert!(is_degrees(parts[1], coordinate.latitude), "{}", name);
                    prop_assert_eq!(parts[2], "WGS84[dd.ddddd]");
                }
                Place::Location(_) => prop_assert!(false, "the strategy makes no locations"),
            }
        }

        let request = network(Protocol::Motis).trips_request(&query).unwrap();
        prop_assert_eq!(request.method, Method::Get);
        prop_assert!(
            request.url.starts_with("https://example.org/base/v6/plan?"),
            "{}",
            request.url
        );
        for (place, name) in [(&from, "fromPlace"), (&to, "toPlace")] {
            let value = parameter(&request, name);
            match place {
                Place::Id(id) => prop_assert_eq!(value, [id.clone()]),
                Place::Coordinate(coordinate) => {
                    let value = value.concat();
                    let parts: Vec<_> = value.split(',').collect();
                    prop_assert_eq!(parts.len(), 2, "{}", value);
                    prop_assert!(is_degrees(parts[0], coordinate.latitude), "{}", value);
                    prop_assert!(is_degrees(parts[1], coordinate.longitude), "{}", value);
                }
                Place::Location(_) => prop_assert!(false, "the strategy makes no locations"),
            }
        }
    }

    /// MOTIS wants the cursor next to the request that it answered.
    #[test]
    fn motis_sends_the_page_as_the_cursor_and_keeps_the_rest_of_the_query(
        page in ".+",
        arrival in prop::option::of(any::<bool>()),
    ) {
        let query = TripsQuery::new(Place::Id("a".into()), Place::Id("b".into())).with_limit(3);
        let query = match arrival {
            Some(true) => query.with_arrival(utc(13, 24)),
            Some(false) => query.with_departure(utc(13, 24)),
            None => query,
        };
        let network = network(Protocol::Motis);
        let without = network.trips_request(&query).unwrap();
        let with = network.trips_request(&query.with_page(page.clone())).unwrap();

        prop_assert_eq!(parameter(&with, "pageCursor"), [page]);
        let mut rest = parameters(&with);
        rest.retain(|(name, _)| name != "pageCursor");
        prop_assert_eq!(rest, parameters(&without));
    }

    #[test]
    fn parsing_never_panics(
        status in 200_u16..300,
        body in prop_oneof![
            any::<Vec<u8>>(),
            (0..VVO.len()).prop_map(|end| VVO[..end].to_vec()),
            (0..VRR.len()).prop_map(|end| VRR[..end].to_vec()),
            (0..TRANSITOUS.len()).prop_map(|end| TRANSITOUS[..end].to_vec()),
        ],
    ) {
        for protocol in [Protocol::Efa, Protocol::Motis] {
            let _ = network(protocol).parse_trips(&HttpResponse {
                status,
                headers: Vec::new(),
                body: body.clone(),
            });
        }
    }
}

#[test]
fn efa_request_sends_the_local_time_with_its_meaning_and_omits_what_the_query_leaves_open() {
    let network = network(Protocol::Efa).with_timezone("Europe/Berlin");
    let query = TripsQuery::new(
        Place::Id("de:14612:28".into()),
        Place::Id("de:14612:13".into()),
    );
    let request = network.trips_request(&query).unwrap();
    for (name, value) in [
        ("outputFormat", "rapidJSON"),
        ("coordOutputFormat", "WGS84[dd.ddddd]"),
        ("useRealtime", "1"),
        // Without it the server adds a trip from the other side of the time.
        ("calcOneDirection", "1"),
    ] {
        assert_eq!(parameter(&request, name), [value], "{name}");
    }
    for name in [
        "calcNumberOfTrips",
        "itdDate",
        "itdTime",
        "itdTripDateTimeDepArr",
    ] {
        assert_eq!(parameter(&request, name), Vec::<String>::new(), "{name}");
    }

    let time = Timestamp {
        unix_seconds: 1_794_835_440,
    };
    for (query, meaning) in [
        (query.clone().with_departure(time), "dep"),
        (query.with_arrival(time), "arr"),
    ] {
        let request = network.trips_request(&query.with_limit(3)).unwrap();
        assert_eq!(parameter(&request, "calcNumberOfTrips"), ["3"]);
        assert_eq!(parameter(&request, "itdDate"), ["20261116"]);
        assert_eq!(parameter(&request, "itdTime"), ["1424"]);
        assert_eq!(parameter(&request, "itdTripDateTimeDepArr"), [meaning]);
    }
}

#[test]
fn efa_request_with_a_time_needs_a_time_zone() {
    let query = TripsQuery::new(Place::Id("a".into()), Place::Id("b".into()))
        .with_arrival(Timestamp { unix_seconds: 0 });
    let result = network(Protocol::Efa).trips_request(&query);
    assert!(
        matches!(result, Err(Error::InvalidNetwork { .. })),
        "{result:?}"
    );
}

#[test]
fn motis_request_sends_the_time_in_utc_and_marks_only_an_arrival_time() {
    let network = network(Protocol::Motis);
    let query = TripsQuery::new(
        Place::Id("de-DELFI_de:14612:28".into()),
        Place::Id("de-DELFI_de:14612:13".into()),
    );
    let request = network.trips_request(&query).unwrap();
    for name in ["numItineraries", "maxItineraries", "time", "arriveBy"] {
        assert_eq!(parameter(&request, name), Vec::<String>::new(), "{name}");
    }

    let time = Timestamp {
        unix_seconds: 1_794_835_440,
    };
    let request = network
        .trips_request(&query.clone().with_departure(time).with_limit(3))
        .unwrap();
    // The first alone is the least number: a page holds every trip of
    // two hours without the second.
    assert_eq!(parameter(&request, "numItineraries"), ["3"]);
    assert_eq!(parameter(&request, "maxItineraries"), ["3"]);
    assert_eq!(parameter(&request, "time"), ["2026-11-16T13:24:00Z"]);
    assert_eq!(parameter(&request, "arriveBy"), Vec::<String>::new());

    let request = network.trips_request(&query.with_arrival(time)).unwrap();
    assert_eq!(parameter(&request, "time"), ["2026-11-16T13:24:00Z"]);
    assert_eq!(parameter(&request, "arriveBy"), ["true"]);
}

#[test]
fn trips_keep_the_servers_order_and_run_forward_in_time() {
    let fixtures: [Fixture; 3] = [
        (
            Protocol::Efa,
            VVO,
            // Changing at Bahnhof Neustadt is a walk of its own, as in MOTIS.
            &[
                &["RE1", "walk", "6"],
                &["RE1", "walk"],
                &["3"],
                &["S1", "walk", "11"],
            ],
        ),
        (
            Protocol::Efa,
            VRR,
            &[&["RE 1 (RRX)"], &["RE 11 (RRX)"], &["ICE 641"]],
        ),
        (
            Protocol::Motis,
            TRANSITOUS,
            &[
                &["3"],
                &["S1", "walk"],
                &["S1", "walk", "11"],
                &["7"],
                &["8"],
                &["3"],
            ],
        ),
    ];
    for (protocol, body, expected) in fixtures {
        let trips = parse(protocol, body).unwrap().trips;
        let labels: Vec<Vec<&str>> = trips
            .iter()
            .map(|trip| trip.legs.iter().map(label).collect())
            .collect();
        assert_eq!(labels, expected);

        for trip in &trips {
            // Every scheduled time of the trip, in the order it is traveled.
            let mut scheduled = Vec::new();
            for leg in &trip.legs {
                scheduled.push(leg.departure.scheduled_time);
                for stopover in &leg.intermediate_stops {
                    scheduled.extend(stopover.scheduled_arrival);
                    scheduled.extend(stopover.scheduled_departure);
                }
                scheduled.push(leg.arrival.scheduled_time);
            }
            assert!(scheduled.is_sorted(), "{trip:?}");
        }
    }
}

#[test]
fn efa_reads_a_ride_with_its_calls_and_a_walk() {
    let trips = parse(Protocol::Efa, VVO).unwrap().trips;
    let [train, walk] = trips[1].legs.as_slice() else {
        panic!("{:?}", trips[1]);
    };

    let LegKind::Transit { line, direction } = &train.kind else {
        panic!("{:?}", train.kind);
    };
    assert_eq!(line.product.as_deref(), Some("Zug"));
    assert_eq!(direction.as_deref(), Some("Zgorzelec"));
    assert!(!train.cancelled);

    let stop = &train.departure.stop;
    assert_eq!(stop.id, "de:14612:28:8:Gleis19");
    assert_eq!(stop.global_ids, [ifopt("de:14612:28:8:Gleis19")]);
    assert_eq!(stop.name, "Hauptbahnhof");
    assert_eq!(stop.locality.as_deref(), Some("Dresden"));
    assert_eq!(stop.timezone.as_deref(), Some("Europe/Berlin"));
    assert_eq!(
        end_times(&train.departure),
        (utc(13, 23), Some(utc(13, 24)))
    );
    assert_eq!(train.departure.expected_time(), utc(13, 24));
    assert_eq!(platforms(&train.departure), (Some("14"), Some("14")));

    // The stop sequence also lists the two ends of the leg.
    let [call] = train.intermediate_stops.as_slice() else {
        panic!("{:?}", train.intermediate_stops);
    };
    assert_eq!(call.stop.id, "de:14612:1:1:Gleis1");
    assert_eq!(call.stop.name, "Bahnhof Mitte");
    assert_eq!(
        times(call),
        [
            Some(utc(13, 27)),
            Some(utc(13, 29)),
            Some(utc(13, 27)),
            Some(utc(13, 29))
        ]
    );
    assert_eq!(call.scheduled_platform.as_deref(), Some("1"));
    assert_eq!(call.realtime_platform.as_deref(), Some("1"));

    assert_eq!(train.arrival.stop.id, "de:14612:16:1:Gleis2");
    assert_eq!(end_times(&train.arrival), (utc(13, 30), Some(utc(13, 32))));
    assert_eq!(platforms(&train.arrival), (Some("2"), Some("2")));

    assert_eq!(walk.kind, LegKind::Walk);
    assert!(!walk.cancelled);
    assert_eq!(walk.intermediate_stops, []);
    assert_eq!(walk.departure.stop.id, "de:14612:16:1:Gleis2");
    // The server carries the train's delay on into the walk.
    assert_eq!(end_times(&walk.departure), (utc(13, 30), Some(utc(13, 32))));
    assert_eq!(walk.departure.expected_time(), utc(13, 32));
    // The walk ends at the station, not at one of its platforms.
    assert_eq!(walk.arrival.stop.id, "de:14612:13");
    assert_eq!(walk.arrival.stop.name, "Albertplatz");
    assert_eq!(walk.arrival.stop.locality.as_deref(), Some("Dresden"));
    assert_eq!(end_times(&walk.arrival), (utc(13, 40), Some(utc(13, 42))));
    assert_eq!(platforms(&walk.arrival), (None, None));
    assert_eq!(trips[1].arrival().expected_time(), utc(13, 42));
}

/// A change of platform comes as `footPathInfo` on the ride before it. The
/// RE1 is two minutes late, and its platforms are known.
#[test]
fn efa_turns_the_footpath_after_a_ride_into_a_walk_to_the_next_leg() {
    let trips = parse(Protocol::Efa, VVO).unwrap().trips;
    let [train, walk, tram] = trips[0].legs.as_slice() else {
        panic!("{:?}", trips[0]);
    };
    assert_eq!(walk.kind, LegKind::Walk);
    assert!(!walk.cancelled && !walk.departure.cancelled && !walk.arrival.cancelled);
    assert_eq!(walk.intermediate_stops, []);

    assert_eq!(walk.departure.stop, train.arrival.stop);
    assert_eq!(walk.departure.stop.id, "de:14612:16:1:Gleis2");
    assert_eq!(end_times(&walk.departure), (utc(13, 30), Some(utc(13, 32))));
    assert_eq!(platforms(&walk.departure), (None, None));

    assert_eq!(walk.arrival.stop, tram.departure.stop);
    assert_eq!(walk.arrival.stop.id, "de:14612:16:6:1");
    // 240 seconds after the walk starts.
    assert_eq!(end_times(&walk.arrival), (utc(13, 34), Some(utc(13, 36))));
    assert_eq!(platforms(&walk.arrival), (None, None));
}

/// The VRR trains wait at their calls, and the ICE has no predictions.
#[test]
fn efa_tells_the_arrival_at_a_call_from_the_departure() {
    let trips = parse(Protocol::Efa, VRR).unwrap().trips;
    let calls = |trip: usize| -> Vec<_> {
        trips[trip].legs[0]
            .intermediate_stops
            .iter()
            .map(|call| (call.stop.id.as_str(), times(call)))
            .collect()
    };
    assert_eq!(
        calls(0),
        [
            (
                "de:05111:18488:90:1",
                [
                    Some(utc(13, 27)),
                    Some(utc(13, 27)),
                    Some(utc(13, 28)),
                    Some(utc(13, 28))
                ]
            ),
            (
                "de:05112:16032:95:12",
                [
                    Some(utc(13, 36)),
                    Some(utc(13, 36)),
                    Some(utc(13, 38)),
                    Some(utc(13, 38))
                ]
            ),
            (
                "de:05117:15225:90:1",
                [
                    Some(utc(13, 43)),
                    Some(utc(13, 43)),
                    Some(utc(13, 44)),
                    Some(utc(13, 44))
                ]
            ),
        ]
    );
    assert_eq!(
        calls(2),
        [
            (
                "de:05111:18488:90:1",
                [Some(utc(13, 55)), None, Some(utc(13, 57)), None]
            ),
            (
                "de:05112:16032:95:13",
                [Some(utc(14, 5)), None, Some(utc(14, 6)), None]
            ),
        ]
    );
}

#[test]
fn efa_reads_cancelled_legs_platform_changes_and_legs_without_optional_fields() {
    let leg = |class: u8, origin: &str, extra: &str| {
        format!(
            r#"{{"origin":{{"id":"de:14612:28","isGlobalId":true,"name":"Dresden Hauptbahnhof","type":"stop","departureTimePlanned":"2026-11-16T13:24:00Z"{origin}}},"destination":{{"id":"de:14612:13","isGlobalId":true,"name":"Dresden Albertplatz","type":"stop","arrivalTimePlanned":"2026-11-16T13:40:00Z"}},"transportation":{{"number":"3","product":{{"class":{class},"name":"x"}}}}{extra}}}"#
        )
    };
    let body = format!(
        r#"{{"version":"11.0.6.72","journeys":[{{"legs":[{},{},{}]}}]}}"#,
        // Only a footpath after a ride that another leg follows is a walk of
        // its own.
        leg(
            4,
            r#","properties":{"plannedPlatformName":"1","platformName":"2"}"#,
            r#","isCancelled":true,"footPathInfo":[{"position":"BEFORE","duration":60}]"#
        ),
        leg(4, "", r#","realtimeStatus":["MONITORED","TRIP_CANCELLED"]"#),
        leg(
            100,
            "",
            r#","realtimeStatus":["MONITORED"],"footPathInfo":[{"position":"AFTER","duration":60}]"#
        ),
    );
    let trips = parse(Protocol::Efa, body.as_bytes()).unwrap().trips;
    let [changed, cancelled, walk] = trips[0].legs.as_slice() else {
        panic!("{trips:?}");
    };
    assert!(changed.cancelled);
    assert_eq!(platforms(&changed.departure), (Some("1"), Some("2")));
    assert!(cancelled.cancelled);
    assert_eq!(walk.kind, LegKind::Walk, "class 100");
    assert!(!walk.cancelled);
    assert_eq!(walk.intermediate_stops, []);
    assert_eq!(end_times(&walk.departure), (utc(13, 24), None));
    assert_eq!(end_times(&walk.arrival), (utc(13, 40), None));
}

/// A footpath only becomes a walk of its own where one is missing and fits.
#[test]
fn efa_adds_no_transfer_walk_that_overlaps_its_neighbors() {
    let leg = |class: u8, departure: u8, arrival: u8, footpath: &str| {
        format!(
            r#"{{"origin":{{"id":"a","name":"A","type":"stop","departureTimePlanned":"2026-11-16T13:{departure}:00Z"}},"destination":{{"id":"b","name":"B","type":"stop","arrivalTimePlanned":"2026-11-16T13:{arrival}:00Z"}},"transportation":{{"number":"3","product":{{"class":{class}}}}},"footPathInfo":[{footpath}]}}"#
        )
    };
    let kinds = |legs: [String; 2]| {
        let body = format!(
            r#"{{"version":"11.0.6.72","journeys":[{{"legs":[{}]}}]}}"#,
            legs.join(",")
        );
        let trips = parse(Protocol::Efa, body.as_bytes()).unwrap().trips;
        trips[0]
            .legs
            .iter()
            .map(|leg| leg.kind == LegKind::Walk)
            .collect::<Vec<_>>()
    };
    let after = |seconds: i64| format!(r#"{{"position":"AFTER","duration":{seconds}}}"#);
    let ride_then_ride = |footpath: &str| kinds([leg(4, 10, 20, footpath), leg(4, 22, 30, "")]);

    assert_eq!(ride_then_ride(&after(120)), [false, true, false]);
    // The next ride leaves before the walk would end.
    assert_eq!(ride_then_ride(&after(600)), [false, false]);
    assert_eq!(ride_then_ride(&after(-60)), [false, false]);
    // An entry without a duration does not hide a later one.
    let two = format!(r#"{{"position":"AFTER"}},{}"#, after(60));
    assert_eq!(ride_then_ride(&two), [false, true, false]);
    // The server sent the walk as a leg already.
    assert_eq!(
        kinds([leg(4, 10, 20, &after(120)), leg(100, 20, 30, "")]),
        [false, true]
    );
}

/// MOTIS requires the scheduled times on the leg, not on its two places.
#[test]
fn motis_takes_a_missing_scheduled_time_at_an_end_from_the_leg() {
    let body = br#"{"itineraries":[{"legs":[{"mode":"WALK","scheduledStartTime":"2026-11-16T13:20:00Z","scheduledEndTime":"2026-11-16T13:30:00Z","from":{"name":"START","lat":51.04,"lon":13.73},"to":{"name":"END","lat":51.05,"lon":13.74}}]}]}"#;
    let trips = parse(Protocol::Motis, body).unwrap().trips;
    let leg = &trips[0].legs[0];
    assert_eq!(leg.departure.scheduled_time, utc(13, 20));
    assert_eq!(leg.arrival.scheduled_time, utc(13, 30));

    let without = String::from_utf8_lossy(body).replace("scheduledEndTime", "x");
    let result = parse(Protocol::Motis, without.as_bytes());
    assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
}

#[test]
fn leg_without_a_scheduled_time_at_one_end_is_a_parse_error() {
    let parse = |departure: &str, arrival: &str| {
        let body = format!(
            r#"{{"version":"11.0.6.72","journeys":[{{"legs":[{{"origin":{{"id":"de:14612:28","isGlobalId":true,"name":"Dresden Hauptbahnhof","type":"stop","{departure}":"2026-11-16T13:24:00Z"}},"destination":{{"id":"de:14612:13","isGlobalId":true,"name":"Dresden Albertplatz","type":"stop","{arrival}":"2026-11-16T13:40:00Z"}},"transportation":{{"number":"3","product":{{"class":4,"name":"x"}}}}}}]}}]}}"#
        );
        parse(Protocol::Efa, body.as_bytes())
    };
    let complete = parse("departureTimePlanned", "arrivalTimePlanned");
    assert!(complete.is_ok(), "{complete:?}");
    // A predicted time does not stand in for the scheduled one.
    for (departure, arrival) in [
        ("departureTimeEstimated", "arrivalTimePlanned"),
        ("departureTimePlanned", "arrivalTimeEstimated"),
    ] {
        let result = parse(departure, arrival);
        assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
    }
}

#[test]
fn motis_reads_a_ride_with_its_calls_and_a_walk_without_predictions() {
    let trips = parse(Protocol::Motis, TRANSITOUS).unwrap().trips;
    let [train, walk] = trips[1].legs.as_slice() else {
        panic!("{:?}", trips[1]);
    };

    let LegKind::Transit { line, direction } = &train.kind else {
        panic!("{:?}", train.kind);
    };
    assert_eq!(line.product, None);
    assert_eq!(line.operator.as_deref(), Some("DB Regio AG Südost"));
    assert_eq!(direction.as_deref(), Some("Meißen S-Bf. Triebischtal"));
    assert!(!train.cancelled);

    let stop = &train.departure.stop;
    assert_eq!(stop.id, "de-DELFI_de:14612:28:8:Gleis19");
    assert_eq!(stop.global_ids, [ifopt("de:14612:28:8:Gleis19")]);
    assert_eq!(stop.name, "Dresden Hauptbahnhof");
    assert_eq!(stop.timezone.as_deref(), Some("Europe/Berlin"));
    assert_eq!(
        end_times(&train.departure),
        (utc(13, 30), Some(utc(13, 30)))
    );
    assert_eq!(platforms(&train.departure), (Some("Gl.14"), Some("Gl.14")));

    let calls: Vec<_> = train
        .intermediate_stops
        .iter()
        .map(|call| (call.stop.id.as_str(), times(call)))
        .collect();
    assert_eq!(
        calls,
        [
            (
                // The train is expected a minute early.
                "de-DELFI_de:14612:25:3:Gleis1",
                [
                    Some(utc(13, 32)),
                    Some(utc(13, 31)),
                    Some(utc(13, 32)),
                    Some(utc(13, 32))
                ]
            ),
            (
                "de-DELFI_de:14612:1:1:Gleis1",
                [
                    Some(utc(13, 34)),
                    Some(utc(13, 34)),
                    Some(utc(13, 34)),
                    Some(utc(13, 34))
                ]
            ),
        ]
    );
    assert_eq!(end_times(&train.arrival), (utc(13, 36), Some(utc(13, 36))));
    assert_eq!(platforms(&train.arrival), (Some("Gl. 1"), Some("Gl. 1")));

    assert_eq!(walk.kind, LegKind::Walk);
    assert_eq!(walk.intermediate_stops, []);
    // The walk is not `realTime`, so its `departure` and `arrival` are no
    // predictions.
    assert_eq!(end_times(&walk.departure), (utc(13, 36), None));
    assert_eq!(walk.arrival.stop.id, "de-DELFI_de:14612:13:1:1");
    assert_eq!(end_times(&walk.arrival), (utc(13, 45), None));
    assert_eq!(platforms(&walk.arrival), (None, None));
}

#[test]
fn motis_reads_coordinate_ends_other_modes_cancellations_and_platform_changes() {
    let body = br#"{"from":{"name":"START","lat":51.04,"lon":13.73,"vertexType":"NORMAL"},"to":{"name":"Dresden Albertplatz","stopId":"de-DELFI_de:14612:13","lat":51.0633,"lon":13.7467,"tz":"Europe/Berlin","vertexType":"TRANSIT"},"direct":[],"itineraries":[{"duration":1200,"startTime":"2026-11-16T13:20:00Z","endTime":"2026-11-16T13:40:00Z","transfers":0,"legs":[
        {"mode":"BIKE","from":{"name":"START","lat":51.04,"lon":13.73,"tz":"Europe/Berlin","departure":"2026-11-16T13:20:00Z","scheduledDeparture":"2026-11-16T13:20:00Z","vertexType":"START"},"to":{"name":"Dresden Hauptbahnhof","stopId":"de-DELFI_de:14612:28:2:3","lat":51.03975,"lon":13.733516,"tz":"Europe/Berlin","arrival":"2026-11-16T13:25:00Z","scheduledArrival":"2026-11-16T13:25:00Z","vertexType":"TRANSIT"},"duration":300,"startTime":"2026-11-16T13:20:00Z","endTime":"2026-11-16T13:25:00Z","scheduledStartTime":"2026-11-16T13:20:00Z","scheduledEndTime":"2026-11-16T13:25:00Z","realTime":false,"scheduled":false,"distance":900},
        {"mode":"TRAM","from":{"name":"Dresden Hauptbahnhof","stopId":"de-DELFI_de:14612:28:2:3","lat":51.03975,"lon":13.733516,"tz":"Europe/Berlin","departure":"2026-11-16T13:31:00Z","scheduledDeparture":"2026-11-16T13:29:00Z","scheduledTrack":"3","track":"4","vertexType":"TRANSIT","cancelled":false},"to":{"name":"Dresden Albertplatz","stopId":"de-DELFI_de:14612:13:1:2","lat":51.063164,"lon":13.746515,"tz":"Europe/Berlin","arrival":"2026-11-16T13:40:00Z","scheduledArrival":"2026-11-16T13:42:00Z","scheduledTrack":"2","track":"2","vertexType":"TRANSIT","cancelled":false},"duration":540,"startTime":"2026-11-16T13:31:00Z","endTime":"2026-11-16T13:40:00Z","scheduledStartTime":"2026-11-16T13:29:00Z","scheduledEndTime":"2026-11-16T13:42:00Z","realTime":true,"scheduled":true,"headsign":"Wilder Mann","agencyName":"DVB","routeShortName":"3","cancelled":true}
    ]}],"previousPageCursor":"a","nextPageCursor":"b"}"#;
    let trips = parse(Protocol::Motis, body).unwrap().trips;
    let [bike, tram] = trips[0].legs.as_slice() else {
        panic!("{trips:?}");
    };
    assert_eq!(trips[0].transfers(), 0, "the bike is no ride");

    assert_eq!(bike.kind, LegKind::Other);
    assert!(!bike.cancelled);
    // A place without a stop gets an id that MOTIS accepts as a place.
    let id: Vec<_> = bike.departure.stop.id.split(',').collect();
    assert_eq!(id.len(), 2, "{id:?}");
    assert!(
        is_degrees(id[0], 51.04) && is_degrees(id[1], 13.73),
        "{id:?}"
    );
    assert_eq!(
        bike.departure
            .stop
            .coordinate
            .map(|c| (c.latitude, c.longitude)),
        Some((51.04, 13.73))
    );
    assert_eq!(bike.arrival.stop.id, "de-DELFI_de:14612:28:2:3");

    assert_eq!(label(tram), "3", "no displayName");
    assert!(tram.cancelled);
    assert_eq!(end_times(&tram.departure), (utc(13, 29), Some(utc(13, 31))));
    assert_eq!(platforms(&tram.departure), (Some("3"), Some("4")));
    assert_eq!(end_times(&tram.arrival), (utc(13, 42), Some(utc(13, 40))));
    assert_eq!(tram.arrival.expected_time(), utc(13, 40));
    assert_eq!(tram.intermediate_stops, []);
}

/// Both fixtures contain tram 3 to Wilder Mann, leaving at 13:29 UTC. The
/// servers disagree on its predicted times, its operator and the names of its
/// stops.
#[test]
fn both_protocols_describe_the_same_ride_alike() {
    let shared = |protocol, body| {
        let trips = parse(protocol, body).unwrap().trips;
        let tram = trips
            .iter()
            .flat_map(|trip| &trip.legs)
            .find(|leg| label(leg) == "3")
            .unwrap();
        let LegKind::Transit { line, direction } = &tram.kind else {
            panic!("{:?}", tram.kind);
        };
        let end = |end: &LegEnd| {
            (
                end.stop.global_ids.clone(),
                end.scheduled_time,
                end.scheduled_platform.clone(),
                end.realtime_platform.clone(),
            )
        };
        let calls: Vec<_> = tram
            .intermediate_stops
            .iter()
            .map(|call| {
                (
                    call.stop.global_ids.clone(),
                    call.scheduled_arrival,
                    call.scheduled_departure,
                    call.scheduled_platform.clone(),
                    call.realtime_platform.clone(),
                )
            })
            .collect();
        (
            line.mode,
            direction.clone(),
            tram.cancelled,
            end(&tram.departure),
            calls,
            end(&tram.arrival),
        )
    };
    let efa = shared(Protocol::Efa, VVO);
    assert_eq!(efa, shared(Protocol::Motis, TRANSITOUS));

    let (mode, direction, cancelled, departure, calls, arrival) = efa;
    assert_eq!(mode, Mode::Tram);
    assert_eq!(direction.as_deref(), Some("Wilder Mann"));
    assert!(!cancelled);
    assert_eq!(calls.len(), 7);
    let platform = |name: &str| Some(name.to_owned());
    assert_eq!(
        departure,
        (
            vec![ifopt("de:14612:28:2:3")],
            utc(13, 29),
            platform("3"),
            platform("3")
        )
    );
    assert_eq!(
        arrival,
        (
            vec![ifopt("de:14612:13:1:2")],
            utc(13, 42),
            platform("2"),
            platform("2")
        )
    );
}

/// Both fixtures contain the S1 to Bahnhof Neustadt, leaving at 13:30 UTC,
/// with a change to tram 11. The servers disagree on how long the change
/// takes.
#[test]
fn both_protocols_describe_the_same_transfer_alike() {
    let shared = |protocol, body, index: usize| {
        let trip = parse(protocol, body).unwrap().trips.swap_remove(index);
        let ends = [trip.departure(), trip.arrival()].map(|end| end.scheduled_time);
        assert_eq!(ends, [utc(13, 30), utc(13, 43)]);
        let legs: Vec<_> = trip
            .legs
            .iter()
            .map(|leg| {
                (
                    label(leg).to_owned(),
                    leg.departure.stop.global_ids.clone(),
                    leg.departure.scheduled_time,
                    leg.arrival.stop.global_ids.clone(),
                )
            })
            .collect();
        (trip.transfers(), legs)
    };
    let efa = shared(Protocol::Efa, VVO, 3);
    assert_eq!(efa, shared(Protocol::Motis, TRANSITOUS, 2));

    let (transfers, legs) = efa;
    assert_eq!(transfers, 1, "the walk is no ride");
    let labels: Vec<_> = legs.iter().map(|leg| leg.0.as_str()).collect();
    assert_eq!(labels, ["S1", "walk", "11"]);
}

/// A successful EFA response carries a warning, which is no failure. Only the
/// cursors of MOTIS can continue a search that found nothing.
#[test]
fn response_without_trips_is_an_empty_list_with_the_cursors_the_server_sent() {
    for (protocol, body, earlier, later) in [
        (
            Protocol::Efa,
            br#"{"version":"11.0.6.72","systemMessages":[{"type":"warning","module":"BROKER","code":-10015,"text":"itp","subType":"itp-monomodal"}]}"#.as_slice(),
            None,
            None,
        ),
        (
            Protocol::Motis,
            br#"{"direct":[],"itineraries":[],"previousPageCursor":"a","nextPageCursor":"b"}"#.as_slice(),
            Some("a"),
            Some("b"),
        ),
    ] {
        let found = parse(protocol, body).unwrap();
        assert_eq!(
            (found.trips, found.earlier.as_deref(), found.later.as_deref()),
            (Vec::new(), earlier, later),
            "{protocol:?}"
        );
    }
}

/// EFA, TRIAS and OJP 1.0 have no cursors. The trips of the second response
/// are out of order, and only their predicted times tell which one leaves
/// last and which one arrives first.
#[test]
fn reference_of_a_server_without_cursors_continues_the_search_past_the_trips_at_hand() {
    let journey = |departure: [&str; 2], arrival: [&str; 2]| {
        let ([departure, expected_departure], [arrival, expected_arrival]) = (departure, arrival);
        format!(
            r#"{{"legs":[{{"origin":{{"id":"de:14612:28","isGlobalId":true,"name":"Dresden Hauptbahnhof","type":"stop","departureTimePlanned":"2026-11-16T{departure}:00Z","departureTimeEstimated":"2026-11-16T{expected_departure}:00Z"}},"destination":{{"id":"de:14612:13","isGlobalId":true,"name":"Dresden Albertplatz","type":"stop","arrivalTimePlanned":"2026-11-16T{arrival}:00Z","arrivalTimeEstimated":"2026-11-16T{expected_arrival}:00Z"}},"transportation":{{"number":"3","product":{{"class":4,"name":"x"}}}},"realtimeStatus":["MONITORED"]}}]}}"#
        )
    };
    let out_of_order = format!(
        r#"{{"version":"11.0.6.72","journeys":[{},{}]}}"#,
        journey(["13:10", "13:50"], ["14:40", "14:40"]),
        journey(["13:20", "13:20"], ["14:50", "14:30"]),
    );
    let query = TripsQuery::new(
        Place::Id("de:14612:28".into()),
        Place::Id("de:14612:13".into()),
    )
    .with_limit(3);
    // The time of the query that is continued does not count.
    let continued = query.clone().with_arrival(utc(13, 24));
    let after = |time: Timestamp, seconds: i64| Timestamp {
        unix_seconds: time.unix_seconds + seconds,
    };

    for (protocol, body) in [
        (Protocol::Efa, VVO),
        (Protocol::Efa, out_of_order.as_bytes()),
        (Protocol::Trias, TRIAS),
        (Protocol::Ojp1, OJP),
    ] {
        let found = parse(protocol, body).unwrap();
        let latest_departure = found
            .trips
            .iter()
            .map(Trip::departure)
            .map(LegEnd::expected_time)
            .max()
            .unwrap();
        let earliest_arrival = found
            .trips
            .iter()
            .map(Trip::arrival)
            .map(LegEnd::expected_time)
            .min()
            .unwrap();
        let request = |query: TripsQuery| complete_network(protocol).trips_request(&query).unwrap();

        assert_eq!(
            request(continued.clone().with_page(found.later.unwrap())),
            request(query.clone().with_departure(after(latest_departure, 60))),
            "{protocol:?} later"
        );
        assert_eq!(
            request(continued.clone().with_page(found.earlier.unwrap())),
            request(query.clone().with_arrival(after(earliest_arrival, -60))),
            "{protocol:?} earlier"
        );
    }
}

#[test]
fn page_that_the_core_did_not_build_is_an_invalid_query_without_cursors() {
    let cursors = parse(Protocol::Motis, TRANSITOUS).unwrap();
    for page in [
        cursors.earlier.unwrap(),
        cursors.later.unwrap(),
        "not a reference".to_owned(),
    ] {
        let query = TripsQuery::new(Place::Id("a".into()), Place::Id("b".into())).with_page(page);
        for protocol in [Protocol::Efa, Protocol::Trias, Protocol::Ojp1] {
            let result = complete_network(protocol).trips_request(&query);
            assert!(
                matches!(result, Err(Error::InvalidQuery { .. })),
                "{protocol:?}: {result:?}"
            );
        }
    }
}

/// The server repeats the planned times as estimates at the two ends of a
/// leg it has no real-time data for.
#[test]
fn efa_leg_without_realtime_data_has_no_predictions() {
    let trips = parse(Protocol::Efa, VRR).unwrap().trips;
    let ice = trips
        .iter()
        .flat_map(|trip| &trip.legs)
        .find(|leg| label(leg) == "ICE 641")
        .unwrap();
    assert_eq!(ice.departure.realtime_time, None);
    assert_eq!(ice.arrival.realtime_time, None);
}

#[test]
fn motis_keeps_direct_walks_unknown_transit_modes_and_cancelled_stops() {
    let place = |name: &str, extra: &str| {
        format!(
            r#"{{"name":"{name}","stopId":"de-DELFI_de:14612:28:2:3","lat":51.04,"lon":13.73{extra}}}"#
        )
    };
    let body = format!(
        r#"{{"direct":[{{"legs":[{{"mode":"WALK","from":{},"to":{}}}]}}],"itineraries":[{{"legs":[{{"mode":"HOVERCRAFT","routeShortName":"H1","headsign":"Pirna","from":{},"to":{}}}]}}]}}"#,
        place("START", r#","scheduledDeparture":"2026-11-16T13:20:00Z""#),
        place("END", r#","scheduledArrival":"2026-11-16T13:30:00Z""#),
        place(
            "A",
            r#","scheduledDeparture":"2026-11-16T13:20:00Z","cancelled":true"#
        ),
        place("B", r#","scheduledArrival":"2026-11-16T13:30:00Z""#),
    );
    let trips = parse(Protocol::Motis, body.as_bytes()).unwrap().trips;
    let [walk, ride] = trips.as_slice() else {
        panic!("{trips:?}");
    };
    assert_eq!(walk.legs[0].kind, LegKind::Walk);
    assert_eq!(walk.transfers(), 0, "no ride at all");

    let ride = &ride.legs[0];
    let LegKind::Transit { line, direction } = &ride.kind else {
        panic!("{ride:?}");
    };
    assert_eq!((line.name.as_str(), line.mode), ("H1", Mode::Other));
    assert_eq!(direction.as_deref(), Some("Pirna"));
    // The vehicle does not call where the leg starts.
    assert!(ride.departure.cancelled && ride.cancelled);
    assert!(!ride.arrival.cancelled);
}

#[test]
fn coordinate_off_the_globe_is_an_invalid_query() {
    let stop = || Place::Id("de:14612:28".into());
    for (latitude, longitude) in [(91.0, 13.7), (51.0, -180.5), (f64::NAN, 13.7)] {
        let place = Place::Coordinate(Coordinate {
            latitude,
            longitude,
        });
        for query in [
            TripsQuery::new(place.clone(), stop()),
            TripsQuery::new(stop(), place.clone()),
        ] {
            for protocol in [Protocol::Efa, Protocol::Motis] {
                let result = network(protocol).trips_request(&query);
                assert!(
                    matches!(result, Err(Error::InvalidQuery { .. })),
                    "{result:?}"
                );
            }
        }
    }
}

/// EFA answers with status 200 and an error message when a place matches
/// several candidates, MOTIS with status 404 and this body.
#[test]
fn unknown_place_is_not_found() {
    let body = br#"{"version":"11.0.6.72","systemMessages":[{"type":"error","module":"BROKER","code":-8011,"text":"origin: multiple matches"}]}"#;
    let result = parse(Protocol::Efa, body);
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");

    // An error that is not about matching a place means there is no trip.
    let body = br#"{"version":"11.0.6.72","systemMessages":[{"type":"error","module":"PT_KERNEL","code":-4007,"text":"origin too close to destination"}]}"#;
    assert_eq!(
        parse(Protocol::Efa, body).map(|found| found.trips),
        Ok(Vec::new())
    );

    let response = HttpResponse {
        status: 404,
        headers: Vec::new(),
        body: br#"{"error":"Could not find timetable location \"de-DELFI_de:14612:99999\""}"#
            .to_vec(),
    };
    let result = network(Protocol::Motis).parse_trips(&response);
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");
}

#[test]
fn success_status_with_a_foreign_body_is_a_parse_error() {
    for (protocol, other_protocol_body) in [(Protocol::Efa, TRANSITOUS), (Protocol::Motis, VVO)] {
        for body in [other_protocol_body, b"<html>Maintenance</html>", b"{}", b""] {
            let result = parse(protocol, body);
            assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
        }
    }
}
