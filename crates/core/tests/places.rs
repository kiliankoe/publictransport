//! Plain values and locations where a query asks for a stop or a place.

use proptest::prelude::*;
use publictransport::{
    Coordinate, DeparturesQuery, Error, GlobalId, HttpResponse, IdScheme, Location, LocationKind,
    LocationQuery, Network, Place, Protocol, Timestamp, TripsQuery,
};

const PROTOCOLS: [Protocol; 3] = [Protocol::Efa, Protocol::Motis, Protocol::Trias];

/// 2026-11-16T13:24:00Z.
const TIME: Timestamp = Timestamp {
    unix_seconds: 1_794_835_440,
};

const DRESDEN: Coordinate = Coordinate {
    latitude: 51.04,
    longitude: 13.73,
};

fn network(protocol: Protocol) -> Network {
    Network::new("test", "Test", "https://example.org/base", protocol)
        .with_timezone("Europe/Berlin")
        // TRIAS builds no request without one.
        .with_requestor_ref("test")
}

/// A location to assign fields to; it cannot be built outside the crate.
fn location() -> Location {
    let body = br#"[{"type":"STOP","tokens":[[0,4]],"name":"Wien Hauptbahnhof","id":"x","lat":48.18,"lon":16.37,"areas":[],"score":-3.5}]"#;
    let response = HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    };
    let Ok(mut locations) = Network::new("other", "Other", "https://example.org", Protocol::Motis)
        .parse_locations(&publictransport::LocationQuery::new("text"), &response)
    else {
        unreachable!("the body is a MOTIS response");
    };
    locations.remove(0)
}

fn location_in(timezone: Option<&str>) -> Location {
    let mut location = location();
    location.timezone = timezone.map(str::to_owned);
    location
}

prop_compose! {
    /// Stops and other places of the asked network, of another one and of
    /// none, with and without a global ID and a coordinate.
    fn any_location()(
        origin in prop::sample::select(vec![Some("test"), Some("other"), None]),
        id in "[a-z0-9]{1,8}",
        global_id in prop::option::of("[a-z]{2}:[0-9]{1,5}:[0-9]{1,5}(:3:2)?"),
        coordinate in prop::option::of((-90.0..=90.0, -180.0..=180.0)),
        kind in prop::sample::select(vec![LocationKind::Stop, LocationKind::Poi]),
    ) -> Location {
        let mut location = location();
        location.id = id;
        location.global_ids = global_id
            .map(|value| GlobalId { scheme: IdScheme::Ifopt, value })
            .into_iter()
            .collect();
        location.coordinate = coordinate.map(|(latitude, longitude)| Coordinate {
            latitude,
            longitude,
        });
        location.network = origin.map(str::to_owned);
        location.kind = kind;
        location
    }
}

proptest! {
    #[test]
    fn departures_of_a_location_are_asked_as_for_the_stop_id_the_network_gives_it(
        protocol in prop::sample::select(PROTOCOLS.to_vec()),
        location in any_location(),
    ) {
        let network = network(protocol);
        let options = |query: DeparturesQuery| query.with_time(TIME).with_limit(7);
        let asked = network.departures_request(&options(DeparturesQuery::new(&location)));

        match network.stop_id(&location) {
            Some(id) => {
                let by_id = network.departures_request(&options(DeparturesQuery::new(id)));
                prop_assert!(by_id.is_ok(), "{by_id:?}");
                prop_assert_eq!(asked.ok(), by_id.ok());
            }
            None => prop_assert!(matches!(asked, Err(Error::InvalidQuery { .. })), "{asked:?}"),
        }
    }

    #[test]
    fn trips_of_a_location_are_asked_as_for_the_place_the_network_gives_it(
        protocol in prop::sample::select(PROTOCOLS.to_vec()),
        location in any_location(),
        // The location is the origin or the destination.
        reverse: bool,
    ) {
        let network = network(protocol);
        let query = |place: Place| {
            let (origin, destination) = if reverse {
                (Place::Coordinate(DRESDEN), place)
            } else {
                (place, Place::Coordinate(DRESDEN))
            };
            TripsQuery::new(origin, destination).with_departure(TIME).with_limit(3)
        };
        let asked = network.trips_request(&query(Place::Location(location.clone())));

        match network.place(&location) {
            Some(place) => {
                let direct = network.trips_request(&query(place));
                prop_assert!(direct.is_ok(), "{direct:?}");
                prop_assert_eq!(asked.ok(), direct.ok());
            }
            None => prop_assert!(matches!(asked, Err(Error::InvalidQuery { .. })), "{asked:?}"),
        }
    }
}

#[test]
fn local_time_is_what_the_clocks_of_the_zone_show() {
    // (zone, instant, date and time there, seconds ahead of UTC)
    let cases = [
        // The last second of winter time and the first of summer time.
        (
            "Europe/Berlin",
            1_774_745_999,
            (2026, 3, 29, 1, 59, 59),
            3600,
        ),
        ("Europe/Berlin", 1_774_746_000, (2026, 3, 29, 3, 0, 0), 7200),
        // West of UTC, on the day and in the year before.
        (
            "America/New_York",
            1_767_236_400,
            (2025, 12, 31, 22, 0, 0),
            -18_000,
        ),
        // Not a whole hour ahead.
        (
            "Asia/Kolkata",
            TIME.unix_seconds,
            (2026, 11, 16, 18, 54, 0),
            19_800,
        ),
    ];
    for (zone, unix_seconds, clock, offset) in cases {
        let local = location_in(Some(zone)).local_time(Timestamp { unix_seconds });
        let shown = local.map(|local| {
            (
                (
                    local.year,
                    local.month,
                    local.day,
                    local.hour,
                    local.minute,
                    local.second,
                ),
                local.utc_offset_seconds,
            )
        });
        assert_eq!(shown, Some((clock, offset)), "{zone} at {unix_seconds}");
    }
}

#[test]
fn local_time_prints_as_the_time_of_day() {
    // 2026-03-29T03:00:00+02:00.
    let local = location_in(Some("Europe/Berlin")).local_time(Timestamp {
        unix_seconds: 1_774_746_000,
    });
    assert_eq!(
        local.map(|local| local.to_string()).as_deref(),
        Some("03:00")
    );
}

#[test]
fn local_time_is_none_without_a_known_time_zone() {
    // `Etc/Unknown` is a name of the time zone database for "not known".
    for timezone in [
        None,
        Some(""),
        Some("Europe/Dresden"),
        Some("+01:00"),
        Some("Etc/Unknown"),
    ] {
        assert_eq!(location_in(timezone).local_time(TIME), None, "{timezone:?}");
    }
}

#[test]
fn local_time_before_the_year_1_is_none() {
    // 0001-01-01T00:00:00Z, which is still the year before in New York.
    let time = Timestamp {
        unix_seconds: -62_135_596_800,
    };
    assert_eq!(location_in(Some("America/New_York")).local_time(time), None);
}

#[test]
fn departures_of_a_coordinate_are_an_invalid_query() {
    for protocol in PROTOCOLS {
        let result = network(protocol).departures_request(&DeparturesQuery::new(DRESDEN));
        assert!(
            matches!(result, Err(Error::InvalidQuery { .. })),
            "{protocol:?}: {result:?}"
        );
    }
}

#[test]
fn coordinate_off_the_globe_is_an_invalid_query_also_when_a_location_holds_it() {
    let off = Coordinate {
        latitude: 91.0,
        longitude: 13.73,
    };
    let mut location = location();
    location.global_ids = Vec::new();
    location.coordinate = Some(off);
    for protocol in PROTOCOLS {
        let network = network(protocol);
        // Otherwise the query would be refused for a location without a place.
        assert_eq!(network.place(&location), Some(Place::Coordinate(off)));
        let result = network.trips_request(&TripsQuery::new(&location, DRESDEN));
        assert!(
            matches!(result, Err(Error::InvalidQuery { .. })),
            "{protocol:?}: {result:?}"
        );
    }
}

#[test]
fn plain_values_become_the_queries_they_stand_for() {
    let location = location();
    let id = || Place::Id("de:14612:28".to_owned());
    let by_location = || Place::Location(location.clone());

    assert_eq!(Place::from("de:14612:28"), id());
    assert_eq!(Place::from("de:14612:28".to_owned()), id());
    assert_eq!(Place::from(DRESDEN), Place::Coordinate(DRESDEN));
    assert_eq!(Place::from(&location), by_location());
    assert_eq!(Place::from(location.clone()), by_location());

    let departures = DeparturesQuery::new("de:14612:28");
    assert_eq!(departures.stop, id());
    assert_eq!(DeparturesQuery::from("de:14612:28"), departures);
    assert_eq!(DeparturesQuery::from("de:14612:28".to_owned()), departures);
    assert_eq!(DeparturesQuery::from(id()), departures);
    assert_eq!(DeparturesQuery::from(&location).stop, by_location());
    assert_eq!(DeparturesQuery::from(location.clone()).stop, by_location());
    let departures = departures.with_time(TIME).with_limit(7);
    assert_eq!(DeparturesQuery::from(&departures), departures);

    let trips = TripsQuery::new("de:14612:28", &location).with_departure(TIME);
    assert_eq!((&trips.origin, &trips.destination), (&id(), &by_location()));
    assert_eq!(TripsQuery::from(&trips), trips);

    let locations = LocationQuery::new("Hauptbahnhof");
    assert_eq!(LocationQuery::from("Hauptbahnhof"), locations);
    assert_eq!(LocationQuery::from("Hauptbahnhof".to_owned()), locations);
    let locations = locations.with_limit(5);
    assert_eq!(LocationQuery::from(&locations), locations);
}
