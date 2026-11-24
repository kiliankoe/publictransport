//! OTP with its Transmodel API and a geocoder in the Pelias format:
//! requests and parsing, against Entur.

use proptest::prelude::*;
use publictransport::{
    Coordinate, Departure, DeparturesQuery, Error, HttpRequest, HttpResponse, LegKind,
    LocationKind, LocationQuery, Method, Mode, Network, Place, Protocol, RemarkKind, Timestamp,
    TripsQuery,
};
use serde_json::{Value, json};

const GEOCODE: &[u8] = include_bytes!("fixtures/otp/entur-geocode.json");
const DEPARTURES: &[u8] = include_bytes!("fixtures/otp/entur-departures.json");
const UNKNOWN_STOP: &[u8] = include_bytes!("fixtures/otp/entur-departures-unknown-stop.json");
const TRIP: &[u8] = include_bytes!("fixtures/otp/entur-trip.json");

const ENDPOINT: &str = "https://api.entur.io/journey-planner/v3/graphql";
const GEOCODER: &str = "https://api.entur.io/geocoder/v1";

fn entur() -> Network {
    Network::new(
        "entur",
        "Entur",
        "https://api.entur.io/journey-planner/v3/graphql/",
        Protocol::OtpTransmodel,
    )
    .with_geocoder(GEOCODER)
}

fn respond(body: impl AsRef<[u8]>) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.as_ref().to_vec(),
    }
}

/// The body as JSON, `null` if it is none.
fn body(request: &HttpRequest) -> Value {
    let body = request.body.as_deref().unwrap_or_default();
    serde_json::from_slice(body).unwrap_or_default()
}

/// The GraphQL text of a request, empty if it has none.
fn graphql(request: &HttpRequest) -> String {
    let body = body(request);
    body.get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The decoded query parameters of a request, in their order.
fn parameters(request: &HttpRequest) -> Vec<(String, String)> {
    let query = request.url.split_once('?').map_or("", |(_, query)| query);
    form_urlencoded::parse(query.as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}

/// A time on a day of November 2026 in UTC.
fn utc(day: i64, hour: i64, minute: i64, second: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_794_787_200 + (day - 16) * 86400 + hour * 3600 + minute * 60 + second,
    }
}

/// An instant, with the text a request names it as.
fn time() -> impl Strategy<Value = (Timestamp, String)> {
    (1_i64..=30, 0_i64..24, 0_i64..60, 0_i64..60).prop_map(|(day, hour, minute, second)| {
        (
            utc(day, hour, minute, second),
            format!("2026-11-{day:02}T{hour:02}:{minute:02}:{second:02}Z"),
        )
    })
}

/// An end of a trip, with the variable a request names it as. The
/// coordinates are eighths, which a decimal number holds exactly.
fn place() -> impl Strategy<Value = (Place, Value)> {
    prop_oneof![
        "[ -~]{0,20}".prop_map(|id| (Place::Id(id.clone()), json!({"place": id}))),
        (-360..360, -720..720).prop_map(|(y, x): (i32, i32)| {
            let latitude = f64::from(2 * y + 1) / 8.0;
            let longitude = f64::from(2 * x + 1) / 8.0;
            (
                Place::Coordinate(Coordinate {
                    latitude,
                    longitude,
                }),
                json!({"coordinates": {"latitude": latitude, "longitude": longitude}}),
            )
        }),
    ]
}

/// A platform as the server describes it. `details` adds to or replaces
/// its fields.
fn quay(id: &str, details: &Value) -> Value {
    let mut quay = json!({"id": id, "name": id, "latitude": 59.5, "longitude": 10.75});
    if let (Some(quay), Some(details)) = (quay.as_object_mut(), details.as_object()) {
        quay.extend(details.clone());
    }
    quay
}

/// A call of a vehicle at a platform. `details` adds to or replaces its
/// fields.
fn call(quay: &Value, departure: &str, details: &Value) -> Value {
    let mut call = json!({
        "aimedArrivalTime": departure,
        "expectedArrivalTime": departure,
        "aimedDepartureTime": departure,
        "expectedDepartureTime": departure,
        "realtime": true,
        "cancellation": false,
        "destinationDisplay": {"frontText": "Somewhere"},
        "quay": quay,
        "serviceJourney": {"line": line("bus", "localBus")}
    });
    if let (Some(call), Some(details)) = (call.as_object_mut(), details.as_object()) {
        call.extend(details.clone());
    }
    call
}

fn line(mode: &str, submode: &str) -> Value {
    json!({
        "publicCode": "X1",
        "name": "Here - There",
        "transportMode": mode,
        "transportSubmode": submode,
        "authority": {"name": "Operator"}
    })
}

/// The departures that an answer with these calls at a platform holds.
fn parse_calls(network: &Network, calls: &Value) -> Vec<Departure> {
    let answer = json!({"data": {"quay": {"estimatedCalls": calls}}}).to_string();
    network
        .parse_departures(&respond(answer))
        .unwrap_or_default()
}

#[test]
fn departures_and_trips_post_a_graphql_query_to_the_endpoint() {
    let stop = || Place::Id("NSR:StopPlace:59872".into());
    let network = entur();
    let departures = |id: &str| {
        network
            .departures_request(&DeparturesQuery::new(id))
            .unwrap()
    };
    let stop_place = departures("NSR:StopPlace:59872");
    let platform = departures("NSR:Quay:555");
    let trips = network
        .trips_request(&TripsQuery::new(stop(), stop()))
        .unwrap();

    for request in [&stop_place, &platform, &trips] {
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.url, ENDPOINT);
        assert!(
            request.headers.iter().any(|header| {
                header.name.eq_ignore_ascii_case("content-type")
                    && header.value == "application/json"
            }),
            "{:?}",
            request.headers
        );
        assert!(!graphql(request).is_empty(), "{:?}", body(request));
    }
    // A stop place and a platform are different fields of the API.
    // Each kind of place has a field of its own.
    assert!(graphql(&stop_place).contains("stopPlace(id:"));
    assert!(graphql(&platform).contains("quay(id:"));
}

#[test]
fn location_search_needs_a_geocoder_and_a_kind_it_knows() {
    let without = Network::new("entur", "Entur", ENDPOINT, Protocol::OtpTransmodel);
    assert_eq!(without.geocoder, None);
    let result = without.locations_request(&LocationQuery::new("Oslo S"));
    assert!(
        matches!(result, Err(Error::Unsupported { .. })),
        "{result:?}"
    );

    let query = LocationQuery::new("Oslo S").with_kinds([LocationKind::Other]);
    let result = entur().locations_request(&query);
    assert!(
        matches!(result, Err(Error::InvalidQuery { .. })),
        "{result:?}"
    );
}

proptest! {
    #[test]
    fn locations_request_carries_the_query_to_the_geocoder(
        text in ".*",
        limit in prop::option::of(any::<u32>()),
        kinds in prop::collection::vec(
            prop::sample::select(vec![
                LocationKind::Stop,
                LocationKind::Address,
                LocationKind::Poi,
                LocationKind::Other,
            ]),
            0..6,
        ),
        slash in any::<bool>(),
    ) {
        let stop = kinds.contains(&LocationKind::Stop);
        let address = kinds.contains(&LocationKind::Address) || kinds.contains(&LocationKind::Poi);
        prop_assume!(kinds.is_empty() || stop || address);
        let mut query = LocationQuery::new(text.clone()).with_kinds(kinds);
        query.limit = limit;
        let geocoder = if slash { format!("{GEOCODER}/") } else { GEOCODER.to_owned() };

        let request = entur().with_geocoder(geocoder).locations_request(&query).unwrap();

        prop_assert_eq!(request.method, Method::Get);
        let path = request.url.split_once('?').map_or(&*request.url, |(path, _)| path);
        prop_assert_eq!(path, format!("{GEOCODER}/autocomplete"));
        let mut expected = vec![("text".to_owned(), text)];
        if let Some(limit) = limit {
            expected.push(("size".to_owned(), limit.to_string()));
        }
        let layers = match (stop, address) {
            (true, true) => Some("venue,address"),
            (true, false) => Some("venue"),
            (false, true) => Some("address"),
            (false, false) => None,
        };
        if let Some(layers) = layers {
            expected.push(("layers".to_owned(), layers.to_owned()));
        }
        let mut sent = parameters(&request);
        sent.sort();
        expected.sort();
        prop_assert_eq!(sent, expected);
    }

    #[test]
    fn departures_request_carries_the_query(
        stop_id in "[ -~]{0,20}",
        time in prop::option::of(time()),
        limit in prop::option::of(any::<u32>()),
    ) {
        let mut query = DeparturesQuery::new(stop_id.clone());
        query.limit = limit;
        let mut variables = json!({"id": stop_id});
        if let Some(limit) = limit {
            variables["limit"] = json!(limit);
        }
        if let Some((instant, text)) = time {
            query.time = Some(instant);
            variables["start"] = json!(text);
        }

        let request = entur().departures_request(&query).unwrap();
        prop_assert_eq!(&body(&request)["variables"], &variables);
    }

    #[test]
    fn trips_request_carries_the_query(
        (origin, from) in place(),
        (destination, to) in place(),
        time in prop::option::of((time(), any::<bool>())),
        limit in prop::option::of(any::<u32>()),
        page in prop::option::of("[ -~]{1,40}"),
    ) {
        let mut query = TripsQuery::new(origin, destination);
        query.limit = limit;
        query.page.clone_from(&page);
        let mut variables = json!({"from": from, "to": to});
        if let Some(limit) = limit {
            variables["limit"] = json!(limit);
        }
        if let Some(((instant, text), arrive_by)) = time {
            variables["time"] = json!(text);
            query = if arrive_by {
                variables["arriveBy"] = json!(true);
                query.with_arrival(instant)
            } else {
                query.with_departure(instant)
            };
        }
        if let Some(page) = page {
            variables["page"] = json!(page);
        }

        let request = entur().trips_request(&query).unwrap();
        prop_assert_eq!(&body(&request)["variables"], &variables);
    }
}

#[test]
fn coordinate_off_the_globe_is_an_invalid_query() {
    let stop = || Place::Id("NSR:StopPlace:59872".into());
    for (latitude, longitude) in [(91.0, 10.7), (59.9, -180.5), (f64::NAN, 10.7)] {
        let place = Place::Coordinate(Coordinate {
            latitude,
            longitude,
        });
        for query in [
            TripsQuery::new(place.clone(), stop()),
            TripsQuery::new(stop(), place.clone()),
        ] {
            let result = entur().trips_request(&query);
            assert!(
                matches!(result, Err(Error::InvalidQuery { .. })),
                "{result:?}"
            );
        }
    }
}

/// A key for the server must not reach a geocoder of someone else.
#[test]
fn geocoder_gets_the_headers_of_the_network_only_on_the_host_of_the_server() {
    let cases = [
        (GEOCODER, true),
        ("HTTPS://API.ENTUR.IO/geocoder/v1", true),
        ("https://api.entur.io", true),
        ("http://api.entur.io/geocoder/v1", false),
        ("https://api.entur.io:8443/geocoder/v1", false),
        ("https://api.entur.io.example.org/geocoder/v1", false),
        ("https://example.org/api.entur.io/", false),
        ("https://example.org?https://api.entur.io/", false),
        ("//api.entur.io/geocoder/v1", false),
    ];
    for (geocoder, sent) in cases {
        let network = entur()
            .with_geocoder(geocoder)
            .with_header("X-Api-Key", "secret");
        let request = network
            .locations_request(&LocationQuery::new("Oslo"))
            .unwrap();
        let carried = request
            .headers
            .iter()
            .any(|header| header.name == "X-Api-Key");
        assert_eq!(carried, sent, "{geocoder}");
    }
}

#[test]
fn locations_come_in_the_geocoders_order_with_their_kind() {
    let network = entur().with_timezone("Europe/Oslo");
    let locations = network.parse_locations(&respond(GEOCODE)).unwrap();

    let found: Vec<(&str, LocationKind)> = locations.iter().map(|l| (&*l.id, l.kind)).collect();
    assert_eq!(
        found,
        [
            ("NSR:StopPlace:59872", LocationKind::Stop),
            ("NSR:GroupOfStopPlaces:1", LocationKind::Other),
            ("OSM:TopographicPlace:6821148278", LocationKind::Poi),
            ("NSR:StopPlace:58211", LocationKind::Stop),
            ("OSM:TopographicPlace:3132099332", LocationKind::Poi),
        ]
    );
    let first = &locations[0];
    assert_eq!(first.name, "Oslo S");
    assert_eq!(first.locality.as_deref(), Some("Oslo"));
    assert_eq!(first.global_ids, []);
    assert_eq!(first.timezone.as_deref(), Some("Europe/Oslo"));
    assert_eq!(first.network.as_deref(), Some("entur"));
    assert_eq!(
        first.coordinate,
        Some(Coordinate {
            latitude: 59.910_357,
            longitude: 10.753_051
        })
    );
}

#[test]
fn place_of_the_address_layer_without_a_known_category_is_an_address() {
    let feature = |id: &str, properties: Value| {
        let mut feature = json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [10.75, 59.5]},
            "properties": {"id": id, "name": "Karl Johans gate 1", "layer": "address"}
        });
        if let (Some(all), Some(more)) = (
            feature["properties"].as_object_mut(),
            properties.as_object(),
        ) {
            all.extend(more.clone());
        }
        feature
    };
    let answer = json!({"type": "FeatureCollection", "features": [
        feature("a", json!({"category": ["vegadresse"]})),
        feature("b", json!({})),
    ]});

    let locations = entur()
        .parse_locations(&respond(answer.to_string()))
        .unwrap();
    let found: Vec<(&str, LocationKind)> = locations.iter().map(|l| (&*l.id, l.kind)).collect();
    assert_eq!(
        found,
        [("a", LocationKind::Address), ("b", LocationKind::Address)]
    );
    assert_eq!(locations[0].locality, None);
}

#[test]
fn departures_of_a_stop_place_have_platform_line_and_times() {
    let departures = entur().parse_departures(&respond(DEPARTURES)).unwrap();
    assert_eq!(departures.len(), 10);

    let first = &departures[0];
    assert_eq!(first.stop.id, "NSR:Quay:555");
    assert_eq!(first.stop.name, "Oslo S");
    assert_eq!(first.stop.kind, LocationKind::Stop);
    assert_eq!(first.stop.timezone.as_deref(), Some("Europe/Oslo"));
    assert_eq!(first.stop.network.as_deref(), Some("entur"));
    assert_eq!(
        first.stop.coordinate,
        Some(Coordinate {
            latitude: 59.910_54,
            longitude: 10.755_133
        })
    );
    assert_eq!(first.line.name, "L1");
    assert_eq!(first.line.mode, Mode::RegionalTrain);
    assert_eq!(first.line.product, None);
    assert_eq!(first.line.operator.as_deref(), Some("Vy"));
    assert_eq!(first.direction.as_deref(), Some("Lillestrøm"));
    assert_eq!(first.scheduled_time, utc(17, 22, 26, 0));
    assert_eq!(first.realtime_time, Some(utc(17, 22, 26, 4)));
    assert_eq!(first.scheduled_platform.as_deref(), Some("9"));
    assert_eq!(first.realtime_platform.as_deref(), Some("9"));
    assert!(!first.cancelled);

    let cancelled: Vec<_> = departures.iter().filter(|d| d.cancelled).collect();
    let [cancelled] = cancelled.as_slice() else {
        unreachable!("one cancelled departure: {cancelled:?}")
    };
    assert_eq!(cancelled.line.name, "RE10");
    assert_eq!(cancelled.scheduled_time, utc(17, 22, 29, 0));

    let last = &departures[9];
    assert_eq!(last.line.product.as_deref(), Some("airportLinkRail"));
}

/// The server sends the expected time and the platform of a call without
/// real-time data too, as copies of the scheduled ones.
#[test]
fn departures_of_a_platform_are_ordered_and_real_time_only_if_the_call_says_so() {
    let live = quay(
        "NSR:Quay:1",
        &json!({
            "name": "Platform A",
            "publicCode": "2",
            "timeZone": "Europe/Oslo",
            "stopPlace": {"id": "NSR:StopPlace:1", "name": "Station"}
        }),
    );
    let planned = quay("NSR:Quay:2", &json!({"publicCode": "3"}));
    let mut unnamed = line("bus", "localBus");
    unnamed["publicCode"] = Value::Null;
    let calls = json!([
        call(
            &live,
            "2026-11-17T10:00:00Z",
            &json!({
                "expectedDepartureTime": "2026-11-17T11:10:00+01:00",
                "scheduledQuay": {"publicCode": "1"}
            })
        ),
        call(
            &planned,
            "2026-11-17T10:05:00Z",
            &json!({
                "expectedDepartureTime": "2026-11-17T10:20:00Z",
                "realtime": false,
                "serviceJourney": {"line": unnamed}
            })
        ),
    ]);

    let departures = parse_calls(&entur().with_timezone("Europe/Stockholm"), &calls);
    let [planned, live] = departures.as_slice() else {
        unreachable!("two departures: {departures:?}")
    };
    assert_eq!(planned.stop.id, "NSR:Quay:2");
    assert_eq!(planned.stop.name, "NSR:Quay:2");
    assert_eq!(planned.stop.timezone.as_deref(), Some("Europe/Stockholm"));
    assert_eq!(planned.line.name, "Here - There");
    assert_eq!(planned.scheduled_time, utc(17, 10, 5, 0));
    assert_eq!(planned.realtime_time, None);
    assert_eq!(planned.scheduled_platform.as_deref(), Some("3"));
    assert_eq!(planned.realtime_platform, None);

    assert_eq!(live.stop.name, "Station");
    assert_eq!(live.stop.timezone.as_deref(), Some("Europe/Oslo"));
    assert_eq!(live.realtime_time, Some(utc(17, 10, 10, 0)));
    assert_eq!(live.scheduled_platform.as_deref(), Some("1"));
    assert_eq!(live.realtime_platform.as_deref(), Some("2"));
}

#[test]
fn mode_follows_the_transport_mode_and_for_rail_the_submode() {
    let cases = [
        ("air", "domesticFlight", Mode::Airplane),
        ("bus", "localBus", Mode::Bus),
        ("trolleybus", "unknown", Mode::Bus),
        ("coach", "nationalCoach", Mode::Coach),
        ("metro", "metro", Mode::Subway),
        ("tram", "localTram", Mode::Tram),
        ("water", "localCarFerry", Mode::Ferry),
        ("cableway", "telecabin", Mode::CableCar),
        ("lift", "unknown", Mode::CableCar),
        ("funicular", "funicular", Mode::Funicular),
        ("taxi", "communalTaxi", Mode::OnDemand),
        ("rail", "highSpeedRail", Mode::HighSpeedTrain),
        ("rail", "longDistance", Mode::LongDistanceTrain),
        ("rail", "international", Mode::LongDistanceTrain),
        ("rail", "interregionalRail", Mode::LongDistanceTrain),
        ("rail", "nightRail", Mode::NightTrain),
        ("rail", "suburbanRailway", Mode::SuburbanTrain),
        ("rail", "regionalRail", Mode::RegionalTrain),
        ("monorail", "unknown", Mode::Other),
    ];
    for (mode, submode, expected) in cases {
        let details = json!({"serviceJourney": {"line": line(mode, submode)}});
        let calls = json!([call(
            &quay("NSR:Quay:1", &json!({})),
            "2026-11-17T10:00:00Z",
            &details
        )]);
        let found: Vec<Mode> = parse_calls(&entur(), &calls)
            .iter()
            .map(|departure| departure.line.mode)
            .collect();
        assert_eq!(found, [expected], "{mode} {submode}");
    }
}

#[test]
fn trips_have_rides_with_their_stops_and_cursors_for_more() {
    let found = entur().parse_trips(&respond(TRIP)).unwrap();
    assert!(
        found
            .earlier
            .as_deref()
            .is_some_and(|c| c.starts_with("MnxQUkVWSU9VU19QQUdF")),
        "{:?}",
        found.earlier
    );
    assert!(
        found
            .later
            .as_deref()
            .is_some_and(|c| c.starts_with("MnxORVhUX1BBR0V8")),
        "{:?}",
        found.later
    );
    assert_eq!(found.trips.len(), 3);

    let [ride] = found.trips[0].legs.as_slice() else {
        unreachable!("one leg: {:?}", found.trips[0])
    };
    let LegKind::Transit { line, direction } = &ride.kind else {
        unreachable!("a ride: {:?}", ride.kind)
    };
    assert_eq!(line.name, "RE10");
    assert_eq!(line.mode, Mode::RegionalTrain);
    assert_eq!(line.product, None);
    assert_eq!(line.operator.as_deref(), Some("Vy"));
    assert_eq!(direction.as_deref(), Some("Lillehammer"));
    assert!(!ride.cancelled && !ride.departure.cancelled && !ride.arrival.cancelled);

    assert_eq!(ride.departure.stop.id, "NSR:Quay:571");
    assert_eq!(ride.departure.stop.name, "Oslo S");
    assert_eq!(ride.departure.stop.kind, LocationKind::Stop);
    assert_eq!(ride.departure.scheduled_time, utc(17, 22, 34, 0));
    assert_eq!(ride.departure.realtime_time, Some(utc(17, 22, 34, 0)));
    assert_eq!(ride.departure.scheduled_platform.as_deref(), Some("11"));
    assert_eq!(ride.departure.realtime_platform.as_deref(), Some("11"));
    assert_eq!(ride.arrival.stop.id, "NSR:Quay:451");
    assert_eq!(ride.arrival.scheduled_time, utc(17, 22, 57, 0));
    assert_eq!(ride.arrival.realtime_time, Some(utc(17, 22, 57, 0)));
    assert_eq!(ride.arrival.scheduled_platform.as_deref(), Some("1"));
    assert_eq!(ride.arrival.realtime_platform.as_deref(), Some("1"));

    let [call] = ride.intermediate_stops.as_slice() else {
        unreachable!("one call: {:?}", ride.intermediate_stops)
    };
    assert_eq!(call.stop.id, "NSR:Quay:754");
    assert_eq!(call.stop.name, "Lillestrøm stasjon");
    assert_eq!(call.scheduled_arrival, Some(utc(17, 22, 44, 0)));
    assert_eq!(call.realtime_arrival, Some(utc(17, 22, 44, 0)));
    assert_eq!(call.scheduled_departure, Some(utc(17, 22, 45, 0)));
    assert_eq!(call.realtime_departure, Some(utc(17, 22, 45, 0)));
    assert_eq!(call.scheduled_platform.as_deref(), Some("4"));
    assert_eq!(call.realtime_platform.as_deref(), Some("4"));
    assert!(!call.cancelled);
}

#[test]
fn legs_without_a_line_are_walks_or_other_and_a_dropped_call_cancels_the_ride() {
    let platform = |id: &str| quay(id, &json!({"publicCode": "4"}));
    let at_quay =
        |id: &str| json!({"name": id, "latitude": 59.5, "longitude": 10.75, "quay": platform(id)});
    let address = json!({"name": "Origin", "latitude": 59.5, "longitude": 10.75});
    let leg = |mode: &str, from: &Value, to: &Value| {
        json!({
            "mode": mode,
            "aimedStartTime": "2026-11-17T10:00:00Z",
            "expectedStartTime": "2026-11-17T10:02:00Z",
            "aimedEndTime": "2026-11-17T10:30:00Z",
            "expectedEndTime": "2026-11-17T10:32:00Z",
            "realtime": false,
            "fromPlace": from,
            "toPlace": to,
            "intermediateEstimatedCalls": []
        })
    };
    let mut ride = leg("bus", &at_quay("NSR:Quay:1"), &at_quay("NSR:Quay:3"));
    ride["line"] = line("bus", "localBus");
    ride["fromEstimatedCall"] = call(
        &platform("NSR:Quay:1"),
        "2026-11-17T10:00:00Z",
        &json!({"realtime": false, "cancellation": true}),
    );
    ride["toEstimatedCall"] = call(
        &platform("NSR:Quay:3"),
        "2026-11-17T10:30:00Z",
        &json!({"realtime": false}),
    );
    ride["intermediateEstimatedCalls"] = json!([call(
        &platform("NSR:Quay:2"),
        "2026-11-17T10:15:00Z",
        &json!({"realtime": false, "cancellation": true})
    )]);
    // A pattern without legs is no trip.
    let answer = json!({"data": {"trip": {"tripPatterns": [{"legs": []}, {"legs": [
        leg("foot", &address, &at_quay("NSR:Quay:1")),
        ride,
        leg("bicycle", &at_quay("NSR:Quay:3"), &address),
    ]}]}}});

    let found = entur().parse_trips(&respond(answer.to_string())).unwrap();
    assert_eq!((found.earlier, found.later), (None, None));
    let [trip] = found.trips.as_slice() else {
        unreachable!("one trip: {:?}", found.trips)
    };
    let [walk, ride, other] = trip.legs.as_slice() else {
        unreachable!("three legs: {:?}", trip.legs)
    };

    assert_eq!(walk.kind, LegKind::Walk);
    // The server has no ID for a place that is no platform.
    assert_eq!(walk.departure.stop.id, "");
    assert_eq!(walk.departure.stop.name, "Origin");
    assert_eq!(walk.departure.stop.kind, LocationKind::Other);
    assert_eq!(
        walk.departure.stop.coordinate,
        Some(Coordinate {
            latitude: 59.5,
            longitude: 10.75
        })
    );
    assert_eq!(walk.arrival.stop.id, "NSR:Quay:1");
    assert_eq!(walk.departure.scheduled_time, utc(17, 10, 0, 0));
    assert_eq!(walk.arrival.scheduled_time, utc(17, 10, 30, 0));
    assert_eq!(walk.departure.realtime_time, None);
    assert_eq!(walk.departure.scheduled_platform, None);
    assert!(!walk.cancelled && !walk.departure.cancelled);

    assert!(matches!(ride.kind, LegKind::Transit { .. }), "{ride:?}");
    assert!(ride.cancelled && ride.departure.cancelled && !ride.arrival.cancelled);
    assert_eq!(ride.arrival.realtime_time, None);
    assert_eq!(ride.arrival.scheduled_platform.as_deref(), Some("4"));
    assert_eq!(ride.arrival.realtime_platform, None);
    let [call] = ride.intermediate_stops.as_slice() else {
        unreachable!("one call: {:?}", ride.intermediate_stops)
    };
    assert!(call.cancelled);
    assert_eq!(call.scheduled_departure, Some(utc(17, 10, 15, 0)));
    assert_eq!(
        (call.realtime_arrival, call.realtime_departure),
        (None, None)
    );
    assert_eq!(call.realtime_platform, None);

    assert_eq!(other.kind, LegKind::Other);
}

#[test]
fn answers_without_a_result_are_told_apart() {
    let network = entur();

    // Not the answer of a geocoder, as from a mistyped URL.
    let result = network.parse_locations(&respond(r#"{"data": {}}"#));
    assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");

    let failed = json!({
        "errors": [{"message": "Validation error: no such field"}, {"message": "second"}],
        "data": null
    })
    .to_string();
    for result in [
        network.parse_departures(&respond(&failed)).map(|_| ()),
        network.parse_trips(&respond(&failed)).map(|_| ()),
    ] {
        assert!(
            matches!(&result, Err(Error::Server { status: 200, message }) if message.contains("no such field")),
            "{result:?}"
        );
    }

    for answer in [UNKNOWN_STOP, br#"{"data": {"quay": null}}"#.as_slice()] {
        let result = network.parse_departures(&respond(answer));
        assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");
    }

    let unknown = json!({"data": {"trip": {
        "tripPatterns": [],
        "routingErrors": [{"code": "locationNotFound", "inputField": "from"}]
    }}});
    let result = network.parse_trips(&respond(unknown.to_string()));
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");

    // An error next to an empty result explains why it is empty.
    let partial = json!({
        "data": {"stopPlace": {"estimatedCalls": []}},
        "errors": [{"message": "Exception while fetching data"}]
    });
    let result = network.parse_departures(&respond(partial.to_string()));
    assert!(
        matches!(&result, Err(Error::Server { status: 200, message }) if message.contains("fetching")),
        "{result:?}"
    );

    for foreign in ["<html></html>", "{}"] {
        let result = network.parse_departures(&respond(foreign));
        assert!(
            matches!(result, Err(Error::Parse { .. })),
            "{foreign}: {result:?}"
        );
    }

    let none = json!({"data": {"trip": {
        "previousPageCursor": "before", "nextPageCursor": null, "tripPatterns": []
    }}});
    let found = network.parse_trips(&respond(none.to_string())).unwrap();
    assert_eq!(found.trips, []);
    assert_eq!(
        (found.earlier.as_deref(), found.later),
        (Some("before"), None)
    );
}

#[test]
fn empty_platform_code_is_no_platform() {
    let quay = quay("NSR:Quay:555", &json!({"publicCode": ""}));
    let calls = json!([call(&quay, "2026-11-16T14:24:00+01:00", &json!({}))]);

    let platforms: Vec<_> = parse_calls(&entur(), &calls)
        .into_iter()
        .map(|departure| (departure.scheduled_platform, departure.realtime_platform))
        .collect();

    assert_eq!(platforms, [(None, None)]);
}

/// The situation is one that Entur sent for line 5 at Tøyen. A text comes
/// once per language, the operator's first.
#[test]
fn situations_are_disruptions_and_notices_are_hints() {
    let quay = quay("NSR:Quay:555", &json!({}));
    let remarks = json!({
        "situations": [{
            "situationNumber": "RUT:SituationNumber:823910",
            "summary": [
                {"value": "Tøyen: Linje 5 retning sentrum "},
                {"value": "Tøyen: Line 5 towards city centre"}
            ],
            "description": [
                {"value": "På grunn av bygging av ny inngang,  ber vi reisende benytte fremre del av toget ved avstigning. "},
                {"value": "Due to construction of a new entrance, passengers are advised to use the front part of the train when getting off."}
            ],
            "infoLinks": [],
            "validityPeriods": [{"startTime": "2026-10-02T04:00:00+02:00", "endTime": "2026-12-31T23:59:00+01:00"}]
        }],
        "notices": [{"text": "Bestilles senest 1 time før"}]
    });
    let calls = json!([call(&quay, "2026-11-16T14:24:00+01:00", &remarks)]);
    let departures = parse_calls(&entur(), &calls);
    let found: Vec<_> = departures
        .iter()
        .flat_map(|departure| &departure.remarks)
        .map(|remark| {
            (
                remark.kind,
                remark.title.as_deref(),
                remark.text.as_deref(),
                remark.url.as_deref(),
                remark.id.as_deref(),
                remark.valid_from,
                remark.valid_until,
            )
        })
        .collect();

    assert_eq!(
        found,
        [
            (
                RemarkKind::Disruption,
                Some("Tøyen: Linje 5 retning sentrum"),
                Some(
                    "På grunn av bygging av ny inngang,  ber vi reisende benytte fremre del av toget ved avstigning."
                ),
                None,
                Some("RUT:SituationNumber:823910"),
                Some(utc(-29, 2, 0, 0)),
                Some(utc(61, 22, 59, 0)),
            ),
            (
                RemarkKind::Hint,
                None,
                Some("Bestilles senest 1 time før"),
                None,
                None,
                None,
                None,
            ),
        ]
    );
}

/// A remark comes next to a departure, so one of a shape that no recorded
/// answer showed must not cost the departure.
#[test]
fn remark_of_an_unknown_shape_costs_only_itself() {
    let quay = quay("NSR:Quay:555", &json!({}));
    let odd = json!({
        "situations": [null, {"summary": {"value": "x"}, "situationNumber": 5}, {"summary": [{"value": "Kept"}], "validityPeriods": [{"startTime": "soon"}]}],
        "notices": {"text": "x"}
    });
    let calls = json!([call(&quay, "2026-11-16T14:24:00+01:00", &odd)]);

    let titles: Vec<Vec<_>> = parse_calls(&entur(), &calls)
        .into_iter()
        .map(|departure| {
            departure
                .remarks
                .into_iter()
                .map(|remark| remark.title)
                .collect()
        })
        .collect();

    assert_eq!(titles, [[Some("Kept".to_owned())]]);
}
