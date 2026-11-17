//! HAFAS mgate: requests and parsing, against VBB.

use proptest::prelude::*;
use publictransport::{
    Coordinate, DeparturesQuery, Error, GlobalId, HafasOptions, HafasProduct, HttpRequest,
    HttpResponse, IdScheme, LegKind, LocationKind, LocationQuery, Method, Mode, Network, Place,
    Protocol, Registry, Timestamp, TripsQuery,
};
use serde_json::{Value, json};

const LOCATIONS: &[u8] = include_bytes!("fixtures/hafas/vbb-locations.json");
const DEPARTURES: &[u8] = include_bytes!("fixtures/hafas/vbb-departures.json");
const UNKNOWN_STOP: &[u8] = include_bytes!("fixtures/hafas/vbb-departures-unknown-stop.json");
const TRIP: &[u8] = include_bytes!("fixtures/hafas/vbb-trip.json");

const ENDPOINT: &str = "https://fahrinfo.vbb.de/bin/mgate.exe";
const CLIENT: &str = r#"{"type":"WEB","id":"VBB","name":"VBB WebApp","l":"vs_webapp_vbb"}"#;
const AUTH: &str = r#"{"type":"AID","aid":"hafas-vbb-webapp"}"#;

/// VBB's products with the mode each stands for, and one this crate does
/// not know.
const PRODUCTS: [(&str, u32, Mode); 8] = [
    ("suburban", 1, Mode::SuburbanTrain),
    ("subway", 2, Mode::Subway),
    ("tram", 4, Mode::Tram),
    ("bus", 8, Mode::Bus),
    ("ferry", 16, Mode::Ferry),
    ("express", 32, Mode::LongDistanceTrain),
    ("regional", 64, Mode::RegionalTrain),
    ("zeppelin", 128, Mode::Other),
];

fn options() -> HafasOptions {
    HafasOptions::new(CLIENT, AUTH, "1.33")
        .with_ext("VBB.1")
        .with_language("de")
        .with_products(PRODUCTS.iter().map(|(id, bitmask, _)| HafasProduct {
            id: (*id).to_owned(),
            name: id.to_uppercase(),
            bitmasks: vec![*bitmask],
        }))
}

fn bare(options: HafasOptions) -> Network {
    Network::new("vbb", "VBB", ENDPOINT, Protocol::HafasMgate).with_hafas(options)
}

fn vbb() -> Network {
    bare(options()).with_timezone("Europe/Berlin")
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

/// The one service call of a request: its `meth` and `req`.
fn call(request: &HttpRequest) -> Value {
    body(request)
        .pointer("/svcReqL/0")
        .cloned()
        .unwrap_or_default()
}

/// A time on a day of November 2026 in Berlin, where UTC+1 applies.
fn berlin(day: i64, hour: i64, minute: i64, second: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_794_787_200 + (day - 16) * 86400 + hour * 3600 + minute * 60 + second
            - 3600,
    }
}

fn ifopt(value: &str) -> GlobalId {
    GlobalId {
        scheme: IdScheme::Ifopt,
        value: value.to_owned(),
    }
}

/// An answer with one departure of a line of the class `cls`.
fn departure_answer(cls: u32, date: &str, time: &str) -> Value {
    json!({"err": "OK", "svcResL": [{"meth": "StationBoard", "err": "OK", "res": {
        "common": {
            "locL": [{
                "lid": "A=1@L=900100003@", "type": "S", "name": "Alexanderplatz",
                "extId": "900100003", "crd": {"x": 13_411_267, "y": 52_521_508}
            }],
            "prodL": [{"name": "X1", "cls": cls, "prodCtx": {"catOutL": "X       "}}],
            "opL": []
        },
        "jnyL": [{
            "date": date, "prodX": 0, "dirTxt": "Somewhere",
            "stbStop": {"locX": 0, "dTimeS": time}
        }]
    }}]})
}

/// A stop number or a full identifier, with the place a request names it
/// as. Some servers have identifiers of their own without an `@`, such as
/// `de:03241:31`.
fn stop() -> impl Strategy<Value = (String, Value)> {
    prop_oneof![
        "[0-9]{1,9}".prop_map(|id| {
            let lid = format!("A=1@L={id}@");
            (id, json!({"lid": lid}))
        }),
        "[A-Za-z0-9=@:. ]{0,12}"
            .prop_filter("no stop number", |id| {
                id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit())
            })
            .prop_map(|id| (id.clone(), json!({"lid": id}))),
    ]
}

/// An end of a trip, with the place a request names it as.
fn place() -> impl Strategy<Value = (Place, Value)> {
    prop_oneof![
        stop().prop_map(|(id, sent)| (Place::Id(id), sent)),
        (-90_000_000..=90_000_000, -180_000_000..=180_000_000).prop_map(|(y, x): (i32, i32)| {
            let coordinate = Coordinate {
                latitude: f64::from(y) / 1e6,
                longitude: f64::from(x) / 1e6,
            };
            (
                Place::Coordinate(coordinate),
                json!({"type": "C", "crd": {"x": x, "y": y}}),
            )
        }),
    ]
}

/// An instant, with the date and the time that Berlin's clocks show then.
fn local_time() -> impl Strategy<Value = (Timestamp, String, String)> {
    (16_i64..30, 0_i64..24, 0_i64..60, 0_i64..60).prop_map(|(day, hour, minute, second)| {
        (
            berlin(day, hour, minute, second),
            format!("202611{day:02}"),
            format!("{hour:02}{minute:02}{second:02}"),
        )
    })
}

#[test]
fn request_posts_the_call_with_the_networks_options_to_the_endpoint() {
    let query = LocationQuery::new("Alexanderplatz").with_limit(5);
    let request = vbb().locations_request(&query).unwrap();

    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, ENDPOINT);
    assert!(
        request.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("content-type") && header.value == "application/json"
        }),
        "{:?}",
        request.headers
    );
    assert_eq!(
        body(&request),
        json!({
            "lang": "de",
            "client": {"type": "WEB", "id": "VBB", "name": "VBB WebApp", "l": "vs_webapp_vbb"},
            "ext": "VBB.1",
            "ver": "1.33",
            "auth": {"type": "AID", "aid": "hafas-vbb-webapp"},
            "svcReqL": [{"meth": "LocMatch", "req": {"input": {
                "loc": {"type": "ALL", "name": "Alexanderplatz?"},
                "maxLoc": 5,
                "field": "S"
            }}}]
        })
    );

    let plain = bare(HafasOptions::new(CLIENT, AUTH, "1.33"));
    let sent = body(&plain.locations_request(&query).unwrap());
    assert_eq!(sent["lang"], "en");
    assert_eq!(sent.get("ext"), None);
}

#[test]
fn network_without_usable_options_cannot_build_a_request() {
    let networks = [
        Network::new("vbb", "VBB", ENDPOINT, Protocol::HafasMgate),
        bare(HafasOptions::new("[]", AUTH, "1.33")),
        bare(HafasOptions::new(CLIENT, "aid", "1.33")),
    ];
    for network in networks {
        let result = network.locations_request(&LocationQuery::new("Alexanderplatz"));
        assert!(
            matches!(result, Err(Error::InvalidNetwork { .. })),
            "{result:?}"
        );
    }
}

/// The answers hold local times, so the request would be sent in vain.
#[test]
fn departures_and_trips_need_the_time_zone_before_the_request_is_sent() {
    let network = bare(options());
    assert!(network.locations_request(&LocationQuery::new("a")).is_ok());

    let stop = || Place::Id("900100003".into());
    let results = [
        network.departures_request(&DeparturesQuery::new("900100003")),
        network.trips_request(&TripsQuery::new(stop(), stop())),
    ];
    for result in results {
        assert!(
            matches!(result, Err(Error::InvalidNetwork { .. })),
            "{result:?}"
        );
    }
}

proptest! {
    #[test]
    fn locations_request_carries_the_query(
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
    ) {
        let mut query = LocationQuery::new(text.clone()).with_kinds(kinds.clone());
        query.limit = limit;
        let request = vbb().locations_request(&query);

        let letters: String = [
            (LocationKind::Stop, 'S'),
            (LocationKind::Address, 'A'),
            (LocationKind::Poi, 'P'),
        ]
        .into_iter()
        .filter(|(kind, _)| kinds.contains(kind))
        .map(|(_, letter)| letter)
        .collect();
        // The protocol has no word for the other kinds alone.
        if letters.is_empty() && !kinds.is_empty() {
            prop_assert!(matches!(request, Err(Error::InvalidQuery { .. })), "{:?}", request);
            return Ok(());
        }
        let request = request.unwrap();
        let kind = if letters.is_empty() { "ALL" } else { letters.as_str() };
        let mut input = json!({"loc": {"type": kind, "name": format!("{text}?")}, "field": "S"});
        if let Some(limit) = limit {
            input["maxLoc"] = json!(limit);
        }
        prop_assert_eq!(call(&request), json!({"meth": "LocMatch", "req": {"input": input}}));
    }

    #[test]
    fn departures_request_carries_the_query(
        (stop_id, sent) in stop(),
        time in prop::option::of(local_time()),
        limit in prop::option::of(any::<u32>()),
    ) {
        let mut query = DeparturesQuery::new(stop_id);
        query.limit = limit;
        let mut req = json!({"type": "DEP", "stbLoc": sent});
        if let Some(limit) = limit {
            req["maxJny"] = json!(limit);
        }
        if let Some((instant, date, time)) = time {
            query.time = Some(instant);
            req["date"] = json!(date);
            req["time"] = json!(time);
        }

        let request = vbb().departures_request(&query).unwrap();
        prop_assert_eq!(call(&request), json!({"meth": "StationBoard", "req": req}));
    }

    #[test]
    fn trips_request_carries_the_query(
        (origin, from) in place(),
        (destination, to) in place(),
        time in prop::option::of((local_time(), any::<bool>())),
        limit in prop::option::of(any::<u32>()),
        page in prop::option::of("[ -~]{1,40}"),
    ) {
        let mut query = TripsQuery::new(origin, destination);
        query.limit = limit;
        query.page.clone_from(&page);
        let mut req = json!({
            "depLocL": [from],
            "arrLocL": [to],
            "outFrwd": true,
            "getPasslist": true
        });
        if let Some(limit) = limit {
            req["numF"] = json!(limit);
        }
        if let Some(((instant, date, time), arrive_by)) = time {
            query = if arrive_by {
                query.with_arrival(instant)
            } else {
                query.with_departure(instant)
            };
            req["outFrwd"] = json!(!arrive_by);
            if page.is_none() {
                req["outDate"] = json!(date);
                req["outTime"] = json!(time);
            }
        }
        if let Some(page) = page {
            req["ctxScr"] = json!(page);
        }

        let request = vbb().trips_request(&query).unwrap();
        prop_assert_eq!(call(&request), json!({"meth": "TripSearch", "req": req}));
    }

    /// The scheduled time is the journey's date plus a time that may count
    /// days in front.
    #[test]
    fn time_is_local_to_the_network_and_may_run_past_midnight(
        day in 16_i64..30,
        days_later in 0_i64..3,
        hour in 0_i64..24,
        minute in 0_i64..60,
        second in 0_i64..60,
    ) {
        let date = format!("202611{day:02}");
        let mut time = format!("{hour:02}{minute:02}{second:02}");
        if days_later > 0 {
            time = format!("{days_later:02}{time}");
        }
        let answer = departure_answer(8, &date, &time).to_string();

        let departures = vbb().parse_departures(&respond(answer)).unwrap();
        prop_assert_eq!(departures.len(), 1);
        prop_assert_eq!(
            departures[0].scheduled_time,
            berlin(day + days_later, hour, minute, second)
        );
        prop_assert_eq!(departures[0].realtime_time, None);
    }

    #[test]
    fn mode_is_the_first_product_that_shares_a_bit_with_the_line(cls in 0_u32..512) {
        let expected = PRODUCTS
            .iter()
            .find(|(_, bitmask, _)| cls & bitmask != 0)
            .map_or(Mode::Other, |(_, _, mode)| *mode);
        let answer = departure_answer(cls, "20261117", "201800").to_string();

        let departures = vbb().parse_departures(&respond(answer)).unwrap();
        prop_assert_eq!(departures[0].line.mode, expected);
    }
}

/// The mode of a line whose product has the given ID.
fn mode_of(id: &str) -> Option<Mode> {
    let product = HafasProduct {
        id: id.to_owned(),
        name: String::new(),
        bitmasks: vec![8],
    };
    let network = bare(options().with_products([product])).with_timezone("Europe/Berlin");
    let answer = departure_answer(8, "20261117", "201800").to_string();
    let departures = network.parse_departures(&respond(answer)).ok()?;
    departures.first().map(|departure| departure.line.mode)
}

/// The entries of transport-apis name the same kind of vehicle in many
/// ways.
#[test]
fn product_ids_of_other_networks_have_a_mode() {
    let cases = [
        ("ice", Mode::HighSpeedTrain),
        ("s-bahn", Mode::SuburbanTrain),
        ("fernbus", Mode::Coach),
        // Numbered where an operator has several products of a kind.
        ("long-distance-train-2", Mode::LongDistanceTrain),
        ("anrufSammelTaxi", Mode::OnDemand),
        ("RE/RB", Mode::RegionalTrain),
    ];
    for (id, mode) in cases {
        assert_eq!(mode_of(id), Some(mode), "{id}");
    }

    // A new ID in the entries shows up here.
    let without = ["other", "car-shuttle-train", "tourismTrain", "train-direct"];
    for network in Registry::bundled().networks() {
        let products = network.hafas.map(|options| options.products);
        for product in products.into_iter().flatten() {
            let known = mode_of(&product.id) != Some(Mode::Other);
            assert_ne!(known, without.contains(&&*product.id), "{}", product.id);
        }
    }
}

#[test]
fn locations_come_in_the_servers_order_with_their_ifopt_id() {
    let locations = vbb().parse_locations(&respond(LOCATIONS)).unwrap();

    let ids: Vec<&str> = locations.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "900100003",
            "900100026",
            "900100005",
            "900100024",
            "900100731"
        ]
    );
    let first = &locations[0];
    assert_eq!(first.kind, LocationKind::Stop);
    assert_eq!(first.name, "S+U Alexanderplatz Bhf (Berlin)");
    assert_eq!(first.locality, None);
    assert_eq!(first.global_ids, [ifopt("de:11000:900100003")]);
    assert_eq!(first.timezone.as_deref(), Some("Europe/Berlin"));
    assert_eq!(first.network.as_deref(), Some("vbb"));
    let coordinate = first.coordinate.expect("a coordinate");
    assert!(
        (coordinate.latitude - 52.521_508).abs() < 1e-9,
        "{coordinate:?}"
    );
    assert!(
        (coordinate.longitude - 13.411_267).abs() < 1e-9,
        "{coordinate:?}"
    );
}

#[test]
fn location_without_a_short_id_keeps_its_identifier() {
    let address = "A=2@O=Alexanderstr. 1@X=13416000@Y=52520000@";
    let answer = json!({"err": "OK", "svcResL": [{"meth": "LocMatch", "err": "OK", "res": {
        "match": {"locL": [
            {"lid": address, "type": "A", "name": "Alexanderstr. 1"},
            // The short ID of a place that is no stop does not name it in a request.
            {"lid": "A=4@O=Fernsehturm@L=9001@", "type": "P", "name": "Fernsehturm", "extId": "9001"},
            {"lid": "A=9@O=Mitte@", "type": "X", "name": "Mitte"}
        ]}
    }}]});

    let locations = vbb().parse_locations(&respond(answer.to_string())).unwrap();
    let found: Vec<(&str, LocationKind)> = locations.iter().map(|l| (&*l.id, l.kind)).collect();
    assert_eq!(
        found,
        [
            (address, LocationKind::Address),
            ("A=4@O=Fernsehturm@L=9001@", LocationKind::Poi),
            ("A=9@O=Mitte@", LocationKind::Other),
        ]
    );
    assert!(locations.iter().all(|l| l.global_ids.is_empty()));
}

#[test]
fn departures_resolve_stop_line_and_operator_from_the_shared_lists() {
    let departures = vbb().parse_departures(&respond(DEPARTURES)).unwrap();
    assert_eq!(departures.len(), 12);
    assert!(
        departures.is_sorted_by_key(publictransport::Departure::expected_time),
        "ordered by expected time"
    );

    let late = departures
        .iter()
        .find(|d| d.scheduled_time == berlin(17, 20, 4, 0))
        .expect("the bus at 20:04");
    assert_eq!(late.stop.id, "900100031");
    assert_eq!(late.stop.global_ids, [ifopt("de:11000:900100031")]);
    assert_eq!(late.line.name, "200");
    assert_eq!(late.line.mode, Mode::Bus);
    assert_eq!(late.line.product.as_deref(), Some("Bus"));
    assert_eq!(
        late.line.operator.as_deref(),
        Some("Berliner Verkehrsbetriebe")
    );
    assert_eq!(
        late.direction.as_deref(),
        Some("S+U Zoologischer Garten via Potsdamer Platz")
    );
    assert_eq!(late.realtime_time, Some(berlin(17, 20, 18, 0)));
    assert_eq!(late.scheduled_platform.as_deref(), Some("Pos. 1"));
    assert_eq!(late.realtime_platform, None);
    assert!(!late.cancelled);

    let cancelled: Vec<_> = departures.iter().filter(|d| d.cancelled).collect();
    let [cancelled] = cancelled.as_slice() else {
        unreachable!("one cancelled departure: {cancelled:?}")
    };
    assert_eq!(cancelled.line.name, "S5");
    assert_eq!(cancelled.line.mode, Mode::SuburbanTrain);
    assert_eq!(
        cancelled.line.operator.as_deref(),
        Some("S-Bahn Berlin GmbH")
    );
    assert_eq!(cancelled.stop.id, "900100003");
    assert_eq!(cancelled.scheduled_time, berlin(17, 20, 18, 0));
    assert_eq!(cancelled.realtime_time, None);
}

#[test]
fn trips_have_rides_with_their_stops_and_references_for_more() {
    let found = vbb().parse_trips(&respond(TRIP)).unwrap();
    assert!(
        found
            .earlier
            .as_deref()
            .is_some_and(|r| r.starts_with("3|OB|")),
        "{:?}",
        found.earlier
    );
    assert!(
        found
            .later
            .as_deref()
            .is_some_and(|r| r.starts_with("3|OF|")),
        "{:?}",
        found.later
    );
    assert_eq!(found.trips.len(), 3);

    let [dropped] = found.trips[0].legs.as_slice() else {
        unreachable!("one leg: {:?}", found.trips[0])
    };
    assert!(dropped.cancelled && dropped.departure.cancelled && dropped.arrival.cancelled);
    assert_eq!(dropped.departure.realtime_time, None);
    assert_eq!(dropped.departure.realtime_platform, None);

    let [ride] = found.trips[1].legs.as_slice() else {
        unreachable!("one leg: {:?}", found.trips[1])
    };
    let LegKind::Transit { line, direction } = &ride.kind else {
        unreachable!("a ride: {:?}", ride.kind)
    };
    assert_eq!(line.name, "S7");
    assert_eq!(line.mode, Mode::SuburbanTrain);
    assert_eq!(line.product.as_deref(), Some("S"));
    assert_eq!(line.operator.as_deref(), Some("S-Bahn Berlin GmbH"));
    assert_eq!(direction.as_deref(), Some("S Potsdam Hauptbahnhof"));
    assert!(!ride.cancelled && !ride.departure.cancelled && !ride.arrival.cancelled);

    assert_eq!(ride.departure.stop.id, "900100003");
    assert_eq!(ride.departure.scheduled_time, berlin(17, 20, 20, 0));
    assert_eq!(ride.departure.realtime_time, Some(berlin(17, 20, 20, 0)));
    assert_eq!(ride.departure.scheduled_platform.as_deref(), Some("4"));
    assert_eq!(ride.departure.realtime_platform.as_deref(), Some("4"));
    assert_eq!(ride.arrival.stop.id, "900023201");
    assert_eq!(ride.arrival.scheduled_time, berlin(17, 20, 33, 0));
    assert_eq!(ride.arrival.realtime_time, Some(berlin(17, 20, 33, 0)));
    assert_eq!(ride.arrival.scheduled_platform.as_deref(), Some("6"));
    assert_eq!(ride.arrival.realtime_platform.as_deref(), Some("6"));

    let calls: Vec<&str> = ride
        .intermediate_stops
        .iter()
        .map(|s| s.stop.id.as_str())
        .collect();
    assert_eq!(
        calls,
        [
            "900100002",
            "900100001",
            "900003201",
            "900003102",
            "900003103"
        ]
    );
    let first = &ride.intermediate_stops[0];
    assert_eq!(first.scheduled_arrival, Some(berlin(17, 20, 21, 0)));
    assert_eq!(first.realtime_arrival, Some(berlin(17, 20, 21, 0)));
    assert_eq!(first.scheduled_departure, Some(berlin(17, 20, 22, 0)));
    assert_eq!(first.realtime_departure, Some(berlin(17, 20, 22, 0)));
}

#[test]
fn sections_that_are_no_rides_become_walks_and_other_legs() {
    let stop =
        |id: &str| json!({"lid": format!("A=1@L={id}@"), "type": "S", "name": id, "extId": id});
    let section = |kind: &str, from: u32, to: u32| {
        json!({
            "type": kind,
            "dep": {"locX": from, "dTimeS": "235800"},
            "arr": {"locX": to, "aTimeS": "01000300"}
        })
    };
    let answer = json!({"err": "OK", "svcResL": [{"meth": "TripSearch", "err": "OK", "res": {
        "common": {"locL": [stop("1"), stop("2"), stop("3")], "prodL": [], "opL": []},
        "outConL": [{
            "date": "20261117",
            "dep": {"locX": 0, "dTimeS": "235800"},
            "arr": {"locX": 2, "aTimeS": "01000300"},
            "secL": [section("WALK", 0, 1), section("BIKE", 1, 2)]
        }]
    }}]});

    let found = vbb().parse_trips(&respond(answer.to_string())).unwrap();
    assert_eq!((found.earlier, found.later), (None, None));
    let [trip] = found.trips.as_slice() else {
        unreachable!("one trip: {:?}", found.trips)
    };
    let kinds: Vec<&LegKind> = trip.legs.iter().map(|leg| &leg.kind).collect();
    assert_eq!(kinds, [&LegKind::Walk, &LegKind::Other]);
    let walk = &trip.legs[0];
    assert_eq!(
        (
            walk.departure.stop.id.as_str(),
            walk.arrival.stop.id.as_str()
        ),
        ("1", "2")
    );
    assert_eq!(walk.departure.scheduled_time, berlin(17, 23, 58, 0));
    assert_eq!(walk.arrival.scheduled_time, berlin(18, 0, 3, 0));
}

#[test]
fn errors_inside_a_successful_response_are_told_apart() {
    let service = |code: &str| {
        // The result of a failed call has no shape to rely on.
        json!({"err": "OK", "svcResL": [{"meth": "X", "err": code, "errTxt": "failed", "res": {}}]})
            .to_string()
    };
    let network = vbb();

    let result = network.parse_departures(&respond(r#"{"err": "AUTH", "errTxt": "denied"}"#));
    assert_eq!(result, Err(Error::Unauthorized));
    for answer in [r#"{"err": "PARSE"}"#.to_owned(), service("PARSE")] {
        let result = network.parse_departures(&respond(&answer));
        assert!(
            matches!(&result, Err(Error::Server { status: 200, message }) if message.contains("PARSE")),
            "{answer}: {result:?}"
        );
    }
    let result = network.parse_departures(&respond(UNKNOWN_STOP));
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");
    // A place of a trip search that the server cannot route from.
    let result = network.parse_trips(&respond(service("H9220")));
    assert!(matches!(result, Err(Error::NotFound { .. })), "{result:?}");
    let result = network.parse_departures(&respond("<html></html>"));
    assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");

    let found = network.parse_trips(&respond(service("H890"))).unwrap();
    assert_eq!(found.trips, []);
    assert_eq!((found.earlier, found.later), (None, None));
    assert_eq!(
        network.parse_locations(&respond(service("NO_MATCH"))),
        Ok(Vec::new())
    );
}

/// Seen on servers with versions 1.16 to 1.88 of the interface.
#[test]
fn other_versions_of_the_interface_spell_fields_differently() {
    // The hour from 2:00 to 3:00 happens twice on this day in Berlin.
    let mut answer = departure_answer(8, "20261025", "023000");
    let result = &mut answer["svcResL"][0]["res"];
    let location = &mut result["common"]["locL"][0];
    location["globalIdL"] = json!([
        {"id": "DBS_8089001", "type": "b"},
        {"id": "de:11000:900100003", "type": "A"}
    ]);
    // A server may send the old and the new spelling at once.
    location["gidL"] = json!(["A×de:11000:900100003"]);
    let stop = &mut result["jnyL"][0]["stbStop"];
    stop["dPlatfS"] = json!("7");
    stop["dPltfS"] = json!({"type": "PL", "txt": "7"});
    stop["dPlatfR"] = json!("5");
    stop["dTZOffset"] = json!(60);
    // After the repeated hour, and without an offset of its own.
    stop["dTimeR"] = json!("031000");

    let departures = vbb()
        .parse_departures(&respond(answer.to_string()))
        .unwrap();
    let departure = &departures[0];
    assert_eq!(departure.stop.global_ids, [ifopt("de:11000:900100003")]);
    assert_eq!(departure.scheduled_platform.as_deref(), Some("7"));
    assert_eq!(departure.realtime_platform.as_deref(), Some("5"));
    // The offset names the second time the clock shows 2:30.
    let second_time = Timestamp {
        unix_seconds: 1_792_891_800,
    };
    assert_eq!(departure.scheduled_time, second_time);
    assert_eq!(
        departure.realtime_time.map(|time| time.unix_seconds),
        Some(second_time.unix_seconds + 40 * 60)
    );
}

#[test]
fn answer_with_times_needs_the_time_zone() {
    let result = bare(options()).parse_departures(&respond(DEPARTURES));
    assert!(
        matches!(result, Err(Error::InvalidNetwork { .. })),
        "{result:?}"
    );
}

#[test]
fn reference_to_a_missing_entry_of_a_shared_list_is_a_parse_error() {
    for (field, index) in [("locX", 1), ("prodX", 1)] {
        let mut answer = departure_answer(8, "20261117", "201800");
        let journey = &mut answer["svcResL"][0]["res"]["jnyL"][0];
        if field == "locX" {
            journey["stbStop"][field] = json!(index);
        } else {
            journey[field] = json!(index);
        }
        let result = vbb().parse_departures(&respond(answer.to_string()));
        assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
    }
}
