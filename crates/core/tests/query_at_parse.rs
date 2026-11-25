//! What parsing makes of an answer with the query that was sent: trips cut
//! to the limit and paged without gaps or repeats, and stops within the
//! radius.

use publictransport::{
    Coordinate, HafasOptions, HttpResponse, Location, NearbyQuery, Network, Place, Protocol,
    Timestamp, Trip, Trips, TripsQuery,
};

const VVO: &[u8] = include_bytes!("fixtures/efa/vvo-trip.json");
const VRR: &[u8] = include_bytes!("fixtures/efa/vrr-trip.json");
const TRIAS: &[u8] = include_bytes!("fixtures/trias/vvo-trip.xml");
const OJP: &[u8] = include_bytes!("fixtures/ojp/vrr-trip.xml");
const TRANSITOUS: &[u8] = include_bytes!("fixtures/motis/transitous-plan.json");

/// The protocols without paging of their own, with a recorded answer each
/// and the number of trips in it.
const WITHOUT_PAGING: [(Protocol, &[u8], usize); 4] = [
    (Protocol::Efa, VVO, 4),
    (Protocol::Efa, VRR, 3),
    (Protocol::Trias, TRIAS, 5),
    (Protocol::Ojp1, OJP, 3),
];

/// 2026-11-16T13:24:00Z, the first departure in `efa/vvo-trip.json`.
const TIME: Timestamp = Timestamp {
    unix_seconds: 1_794_835_440,
};

/// A network that can build and parse everything the tests ask of it.
fn network(protocol: Protocol) -> Network {
    let network = Network::new("test", "Test", "https://example.org/base", protocol)
        .with_timezone("Europe/Berlin")
        .with_requestor_ref("OpenService");
    match protocol {
        Protocol::HafasMgate => network.with_hafas(HafasOptions::new(
            r#"{"type":"WEB","id":"VBB","name":"VBB WebApp","l":"vs_webapp_vbb"}"#,
            r#"{"type":"AID","aid":"hafas-vbb-webapp"}"#,
            "1.33",
        )),
        _ => network,
    }
}

fn respond(body: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

#[expect(clippy::panic, reason = "a fixture that cannot be read fails the test")]
fn parse(protocol: Protocol, query: &TripsQuery, body: &[u8]) -> Trips {
    network(protocol)
        .parse_trips(query, &respond(body))
        .unwrap_or_else(|error| panic!("{protocol:?}: {error}"))
}

fn query() -> TripsQuery {
    TripsQuery::new(Place::Id("a".into()), Place::Id("b".into()))
}

/// For every limit below the number of trips an answer holds, the first page
/// keeps the trips that leave first (or, searching by arrival, arrive last),
/// and the next page, parsed from the same body, holds the trips right after
/// it. The body stands for both answers because a server asked from a time
/// sends the trips from that time on.
#[test]
fn pages_of_a_server_without_paging_neither_skip_nor_repeat_a_trip() {
    for (protocol, body, count) in WITHOUT_PAGING {
        let all = parse(protocol, &query(), body).trips;
        for n in 1..count {
            // Leaving: the earliest departures first, continued by `later`.
            // Arriving: the latest arrivals first, continued by `earlier`.
            for arriving in [false, true] {
                // Sorting by this key puts the trips that page one keeps first.
                let key: fn(&Trip) -> i64 = if arriving {
                    |trip| -trip.arrival().expected_time().unix_seconds
                } else {
                    |trip| trip.departure().expected_time().unix_seconds
                };
                let in_order = |trips: &[Trip]| {
                    let mut keys: Vec<i64> = trips.iter().map(key).collect();
                    keys.sort_unstable();
                    keys
                };
                let limit = u32::try_from(n).unwrap();
                let first_query = if arriving {
                    query().with_arrival(TIME)
                } else {
                    query().with_departure(TIME)
                }
                .with_limit(limit);
                let mut expected = in_order(&all);
                expected.truncate(2 * n);
                let case = format!("{protocol:?}, limit {limit}, arriving {arriving}");

                let page_one = parse(protocol, &first_query, body);
                assert_eq!(
                    in_order(&page_one.trips),
                    expected[..n],
                    "{case}: first page"
                );

                let reference = if arriving {
                    page_one.earlier
                } else {
                    page_one.later
                };
                let next_query = first_query.with_page(reference.expect(&case));
                let page_two = parse(protocol, &next_query, body).trips;
                assert!(
                    page_two.iter().all(|trip| !page_one.trips.contains(trip)),
                    "{case}: the next page repeats a trip"
                );
                let both = [page_one.trips, page_two].concat();
                assert_eq!(in_order(&both), expected, "{case}: the pages skip a trip");
            }
        }
    }
}

/// Without a limit nothing is cut, and protocols with paging of their own
/// keep every trip and the server's references.
#[test]
fn only_a_limit_on_a_server_without_paging_cuts_the_trips() {
    for (protocol, body, count) in WITHOUT_PAGING {
        for query in [
            query(),
            query().with_departure(TIME),
            query().with_arrival(TIME),
        ] {
            let trips = parse(protocol, &query, body).trips;
            assert_eq!(trips.len(), count, "{protocol:?}, {query:?}");
        }
    }

    let unlimited = parse(Protocol::Motis, &query(), TRANSITOUS);
    let limited = parse(Protocol::Motis, &query().with_limit(1), TRANSITOUS);
    assert!(unlimited.trips.len() > 1);
    assert_eq!(limited, unlimited);
    assert!(limited.earlier.is_some() && limited.later.is_some());
}

/// Each fixture with the position it was asked for, as in `nearby.rs`.
const NEARBY: [(Protocol, &[u8], Coordinate); 6] = [
    (
        Protocol::Efa,
        include_bytes!("fixtures/efa/vvo-nearby.json"),
        DRESDEN,
    ),
    (
        Protocol::Motis,
        include_bytes!("fixtures/motis/transitous-nearby.json"),
        DRESDEN,
    ),
    (
        Protocol::Trias,
        include_bytes!("fixtures/trias/vvo-nearby.xml"),
        DRESDEN,
    ),
    (
        Protocol::Ojp1,
        include_bytes!("fixtures/ojp/vrr-nearby.xml"),
        Coordinate {
            latitude: 51.2198,
            longitude: 6.7943,
        },
    ),
    (
        Protocol::HafasMgate,
        include_bytes!("fixtures/hafas/vbb-nearby.json"),
        Coordinate {
            latitude: 52.521_918,
            longitude: 13.413_215,
        },
    ),
    (
        Protocol::OtpTransmodel,
        include_bytes!("fixtures/otp/entur-nearby.json"),
        Coordinate {
            latitude: 59.9109,
            longitude: 10.7530,
        },
    ),
];

const DRESDEN: Coordinate = Coordinate {
    latitude: 51.04,
    longitude: 13.7325,
};

/// A radius just below and just above the distance of every stop: the stop
/// is left out with the one and kept with the other, and the rest keep the
/// server's order.
#[test]
fn stops_farther_than_the_radius_are_left_out() {
    for (protocol, body, position) in NEARBY {
        let nearby = |query: &NearbyQuery| -> Vec<Location> {
            network(protocol)
                .parse_nearby(query, &respond(body))
                .unwrap()
        };
        let all = nearby(&NearbyQuery::new(position).with_radius_meters(100_000));
        assert!(!all.is_empty(), "{protocol:?}");
        let distance = |stop: &Location| position.distance_meters(stop.coordinate.unwrap());

        for stop in &all {
            let meters = distance(stop);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for radius in [meters.floor() as u32, meters.ceil() as u32] {
                let expected: Vec<&str> = all
                    .iter()
                    .filter(|other| distance(other) <= f64::from(radius))
                    .map(|other| other.id.as_str())
                    .collect();
                let found = nearby(&NearbyQuery::new(position).with_radius_meters(radius));
                let found: Vec<&str> = found.iter().map(|other| other.id.as_str()).collect();
                assert_eq!(found, expected, "{protocol:?}, {radius} m");
            }
        }
    }
}

/// Without a radius the query asks for 1000 m, and the EFA fixture holds a
/// stop farther away than that from a position about 1 km east of where it
/// was asked for.
#[test]
fn stops_beyond_the_default_radius_are_left_out() {
    let (protocol, body, asked) = NEARBY[0];
    let all = network(protocol)
        .parse_nearby(
            &NearbyQuery::new(asked).with_radius_meters(100_000),
            &respond(body),
        )
        .unwrap();
    let position = Coordinate {
        latitude: asked.latitude,
        longitude: asked.longitude + 0.015,
    };
    let expected: Vec<&str> = all
        .iter()
        .filter(|stop| position.distance_meters(stop.coordinate.unwrap()) <= 1000.0)
        .map(|stop| stop.id.as_str())
        .collect();
    assert!(expected.len() < all.len());

    let found = network(protocol)
        .parse_nearby(&NearbyQuery::new(position), &respond(body))
        .unwrap();
    let found: Vec<&str> = found.iter().map(|stop| stop.id.as_str()).collect();
    assert_eq!(found, expected);
}

/// A server that counts in minutes, as EFA does: it answers with the first
/// trips of `efa/vvo-trip.json` that leave in the asked minute or later, as
/// many as asked. Two of the four leave in the same minute, so a page of
/// one trip has to get past a minute that holds more trips than it shows.
#[test]
fn paging_through_a_minute_with_more_trips_than_a_page_shows_each_trip_once() {
    let parameter = |url: &str, name: &str| {
        let value = url.split(&format!("{name}=")).nth(1);
        value.and_then(|rest| rest.split('&').next().map(str::to_owned))
    };
    let all: serde_json::Value = serde_json::from_slice(VVO).unwrap();
    // The clock of Berlin, as the request has it: `HHMM`, an hour past UTC.
    let leaves = |journey: &serde_json::Value| {
        let time = journey["legs"][0]["origin"]["departureTimeEstimated"].as_str();
        let (hour, minute) = time.map(|time| (&time[11..13], &time[14..16])).unwrap();
        format!("{:02}{minute}", hour.parse::<u8>().unwrap() + 1)
    };
    let efa = network(Protocol::Efa);
    let mut query = TripsQuery::new("a", "b").with_departure(TIME).with_limit(1);
    let mut shown = Vec::new();
    for _ in 0..4 {
        let url = efa.trips_request(&query).unwrap().url;
        let from = parameter(&url, "itdTime").unwrap();
        let count: usize = parameter(&url, "calcNumberOfTrips")
            .unwrap()
            .parse()
            .unwrap();
        let journeys = all["journeys"].as_array().unwrap().iter();
        let answer: Vec<_> = journeys
            .filter(|journey| leaves(journey) >= from)
            .take(count)
            .collect();
        let body = serde_json::json!({"version": all["version"], "journeys": answer});
        let page = parse(Protocol::Efa, &query, body.to_string().as_bytes());
        shown.extend(page.trips);
        query = query.with_page(page.later.unwrap());
    }
    assert_eq!(
        shown,
        parse(Protocol::Efa, &TripsQuery::new("a", "b"), VVO).trips
    );
}
