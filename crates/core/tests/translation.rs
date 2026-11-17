//! Where a location came from, and how it translates to another network.

use proptest::prelude::*;
use publictransport::{
    Coordinate, GlobalId, HttpResponse, IdScheme, Location, LocationKind, Network, Place, Protocol,
};

const PROTOCOLS: [Protocol; 4] = [
    Protocol::Efa,
    Protocol::Motis,
    Protocol::Trias,
    Protocol::Ojp1,
];

fn network(id: &str, protocol: Protocol) -> Network {
    Network::new(id, "Test", "https://example.org/base", protocol).with_timezone("Europe/Berlin")
}

fn response(body: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

/// A location to assign fields to; it cannot be built outside the crate.
fn location() -> Location {
    let body = br#"[{"type":"STOP","tokens":[[0,4]],"name":"Wien Hauptbahnhof","id":"x","lat":48.18,"lon":16.37,"areas":[],"score":-3.5}]"#;
    let Ok(mut locations) = network("other", Protocol::Motis).parse_locations(&response(body))
    else {
        unreachable!("the body is a MOTIS response");
    };
    locations.remove(0)
}

#[test]
fn every_parsed_location_names_the_network_that_returned_it() {
    type Fixtures = (Protocol, &'static [u8], &'static [u8], &'static [u8]);
    let fixtures: [Fixtures; 4] = [
        (
            Protocol::Efa,
            include_bytes!("fixtures/efa/vvo-stopfinder.json"),
            include_bytes!("fixtures/efa/vvo-departures.json"),
            include_bytes!("fixtures/efa/vvo-trip.json"),
        ),
        (
            Protocol::Motis,
            include_bytes!("fixtures/motis/transitous-geocode.json"),
            include_bytes!("fixtures/motis/transitous-stoptimes.json"),
            include_bytes!("fixtures/motis/transitous-plan.json"),
        ),
        (
            Protocol::Trias,
            include_bytes!("fixtures/trias/vvo-locations.xml"),
            include_bytes!("fixtures/trias/vvo-departures.xml"),
            include_bytes!("fixtures/trias/vvo-trip.xml"),
        ),
        (
            Protocol::Ojp1,
            include_bytes!("fixtures/ojp/vrr-locations.xml"),
            include_bytes!("fixtures/ojp/vrr-departures.xml"),
            include_bytes!("fixtures/ojp/vrr-trip.xml"),
        ),
    ];
    for (protocol, locations, departures, trips) in fixtures {
        let id = format!("{protocol:?}-network");
        let network = network(&id, protocol);
        let check = |operation: &str, found: Vec<Location>| {
            assert!(!found.is_empty(), "{protocol:?} {operation}");
            for location in found {
                assert_eq!(
                    location.network.as_deref(),
                    Some(id.as_str()),
                    "{protocol:?} {operation}: {location:?}"
                );
            }
        };

        check(
            "locations",
            network.parse_locations(&response(locations)).unwrap(),
        );

        let departures = network.parse_departures(&response(departures)).unwrap();
        check(
            "departures",
            departures.into_iter().map(|d| d.stop).collect(),
        );

        let legs = || {
            let trips = network.parse_trips(&response(trips)).unwrap().trips;
            trips.into_iter().flat_map(|trip| trip.legs)
        };
        check(
            "leg ends",
            legs()
                .flat_map(|leg| [leg.departure.stop, leg.arrival.stop])
                .collect(),
        );
        check(
            "intermediate stops",
            legs()
                .flat_map(|leg| leg.intermediate_stops)
                .map(|call| call.stop)
                .collect(),
        );
    }
}

proptest! {
    #[test]
    fn a_location_translates_by_its_id_at_home_and_by_what_is_portable_elsewhere(
        protocol in prop::sample::select(PROTOCOLS.to_vec()),
        origin in prop::sample::select(vec![Some("test"), Some("other"), None]),
        id: String,
        station in "[a-z]{2}:[0-9]{1,5}:[0-9]{1,5}",
        global_id in prop::sample::select(vec![None, Some(""), Some(":3:2")]),
        coordinate in prop::option::of((-90.0..=90.0, -180.0..=180.0)),
        kind in prop::sample::select(vec![LocationKind::Stop, LocationKind::Poi]),
    ) {
        let mut location = location();
        location.id.clone_from(&id);
        location.global_ids = global_id
            .map(|platform| GlobalId {
                scheme: IdScheme::Ifopt,
                value: format!("{station}{platform}"),
            })
            .into_iter()
            .collect();
        location.coordinate = coordinate.map(|(latitude, longitude)| Coordinate {
            latitude,
            longitude,
        });
        location.network = origin.map(str::to_owned);
        location.kind = kind;

        let own = origin == Some("test");
        // MOTIS takes neither foreign IDs nor bare global IDs.
        let by_global_id = protocol != Protocol::Motis && global_id.is_some();
        // Only a stop has departures.
        let stop_id = if kind != LocationKind::Stop {
            None
        } else if own {
            Some(id.clone())
        } else if by_global_id {
            Some(station)
        } else {
            None
        };
        let place = stop_id
            .clone()
            .map(Place::Id)
            .or(location.coordinate.map(Place::Coordinate))
            // At home the ID is the last resort.
            .or((own && !id.is_empty()).then(|| Place::Id(id.clone())));

        let network = network("test", protocol);
        prop_assert_eq!(network.stop_id(&location), stop_id);
        prop_assert_eq!(network.place(&location), place);
    }
}

#[test]
fn a_stop_number_of_the_server_is_no_global_id() {
    let body = String::from_utf8_lossy(include_bytes!("fixtures/trias/vvo-locations.xml"))
        .replace("de:14612:28", "8503000");
    let location = network("test", Protocol::Trias)
        .parse_locations(&response(body.as_bytes()))
        .unwrap()
        .remove(0);
    assert_eq!(location.id, "8503000");
    assert_eq!(location.global_ids, []);
}
