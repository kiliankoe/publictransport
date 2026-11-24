//! The run of one vehicle, for every protocol.

use publictransport::{
    Departure, Error, HafasOptions, HttpRequest, HttpResponse, LegKind, Network, Operation,
    Protocol,
};
use serde_json::Value;

const ENDPOINT: &str = "https://example.org/base";

const PROTOCOLS: [Protocol; 6] = [
    Protocol::Efa,
    Protocol::Motis,
    Protocol::Trias,
    Protocol::Ojp1,
    Protocol::HafasMgate,
    Protocol::OtpTransmodel,
];

/// A protocol, the answer with a departure, the answer with the run of that
/// departure, what the request for the run has to carry, and the number of
/// stops of the run.
type Run<'a> = (
    Protocol,
    &'a [u8],
    &'a [u8],
    &'a [(&'a str, &'a str)],
    usize,
);

/// The identifiers are the ones the server sent with the departure. EFA gets
/// the planned departure, 22:55 UTC, on the clocks of Berlin.
const RUNS: [Run; 6] = [
    (
        Protocol::Efa,
        include_bytes!("fixtures/efa/vvo-journey-departure.json"),
        include_bytes!("fixtures/efa/vvo-journey.json"),
        &[
            ("path", "XML_TRIPSTOPTIMES_REQUEST"),
            ("line", "voe:21066: :H:j26"),
            ("stopID", "de:14612:28"),
            ("tripCode", "639"),
            ("date", "20261123"),
            ("time", "2355"),
        ],
        62,
    ),
    (
        Protocol::Motis,
        include_bytes!("fixtures/motis/transitous-journey-departure.json"),
        include_bytes!("fixtures/motis/transitous-journey.json"),
        &[
            ("path", "v6/trip"),
            ("tripId", "20261123_23:31_de-DELFI_3474164813"),
        ],
        50,
    ),
    (
        Protocol::Trias,
        include_bytes!("fixtures/trias/vvo-journey-departure.xml"),
        include_bytes!("fixtures/trias/vvo-journey.xml"),
        &[
            ("TripInfoRequest/JourneyRef", "voe:21066::H:j26:639"),
            ("TripInfoRequest/OperatingDayRef", "2026-11-23"),
        ],
        62,
    ),
    (
        Protocol::Ojp1,
        include_bytes!("fixtures/ojp/vrr-journey-departure.xml"),
        include_bytes!("fixtures/ojp/vrr-journey.xml"),
        &[
            ("OJPTripInfoRequest/JourneyRef", "ddb:90RE1::H:j26:2"),
            ("OJPTripInfoRequest/OperatingDayRef", "2026-11-23"),
        ],
        12,
    ),
    (
        Protocol::HafasMgate,
        include_bytes!("fixtures/hafas/vbb-journey-departure.json"),
        include_bytes!("fixtures/hafas/vbb-journey.json"),
        &[("meth", "JourneyDetails"), ("jid", "1|62660|6|86|23112026")],
        19,
    ),
    (
        Protocol::OtpTransmodel,
        include_bytes!("fixtures/otp/entur-journey-departure.json"),
        include_bytes!("fixtures/otp/entur-journey.json"),
        &[
            ("field", "serviceJourney"),
            ("id", "VYG:ServiceJourney:2176_902925-R"),
            ("date", "2026-11-23"),
        ],
        26,
    ),
];

/// A network with what its protocol needs to build a request with a time.
fn network(protocol: Protocol) -> Network {
    let network = Network::new("test", "Test", ENDPOINT, protocol);
    match protocol {
        Protocol::Efa => network.with_timezone("Europe/Berlin"),
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

/// A JSON value as the text it stands for.
fn text(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// What a request sends, as names and values. A URL with parameters gives
/// its `path` below the endpoint and the parameters. An XML body gives every
/// element without child elements, named with its parent. HAFAS gives the
/// `meth` and the members of its `req`. GraphQL gives the variables, and the
/// `field` `serviceJourney` if the query asks for it.
fn sent(protocol: Protocol, request: &HttpRequest) -> Vec<(String, String)> {
    let body = request.body.as_deref().unwrap_or_default();
    let json: Value = serde_json::from_slice(body).unwrap_or_default();
    let members = |value: Option<&Value>| -> Vec<(String, String)> {
        let members = value.and_then(Value::as_object).into_iter().flatten();
        members
            .map(|(name, value)| (name.clone(), text(value)))
            .collect()
    };
    match protocol {
        Protocol::Efa | Protocol::Motis => {
            let (path, query) = request
                .url
                .split_once('?')
                .unwrap_or((request.url.as_str(), ""));
            let path = path.strip_prefix(ENDPOINT).unwrap_or(path);
            let mut parts = vec![("path".to_owned(), path.trim_start_matches('/').to_owned())];
            parts.extend(form_urlencoded::parse(query.as_bytes()).into_owned());
            parts
        }
        Protocol::Trias | Protocol::Ojp1 => {
            let document =
                roxmltree::Document::parse(std::str::from_utf8(body).unwrap_or_default());
            document
                .iter()
                .flat_map(roxmltree::Document::descendants)
                .filter(|node| {
                    node.is_element() && !node.children().any(|child| child.is_element())
                })
                .map(|leaf| {
                    let parent = leaf
                        .parent_element()
                        .map_or("", |node| node.tag_name().name());
                    let name = format!("{parent}/{}", leaf.tag_name().name());
                    (name, leaf.text().unwrap_or_default().to_owned())
                })
                .collect()
        }
        Protocol::HafasMgate => {
            let call = json.pointer("/svcReqL/0");
            let mut parts = members(call.and_then(|call| call.get("req")));
            let method = call.and_then(|call| call.get("meth")).map(text);
            parts.extend(method.map(|method| ("meth".to_owned(), method)));
            parts
        }
        _ => {
            let mut parts = members(json.get("variables"));
            // GraphQL ignores whitespace.
            let query = json.get("query").map_or_default(text);
            let query: String = query.chars().filter(|c| !c.is_whitespace()).collect();
            if query.contains("serviceJourney(") {
                parts.push(("field".to_owned(), "serviceJourney".to_owned()));
            }
            parts
        }
    }
}

/// Whether the request sends every one of the names with its value.
fn carries(protocol: Protocol, request: &HttpRequest, expected: &[(&str, &str)]) -> bool {
    let sent = sent(protocol, request);
    expected.iter().all(|(name, value)| {
        sent.iter()
            .any(|(sent_name, sent_value)| sent_name == name && sent_value == value)
    })
}

/// The ID of the run of the departure in the recorded answer for it.
fn recorded_id(protocol: Protocol) -> Option<String> {
    let (_, departure, ..) = RUNS.into_iter().find(|run| run.0 == protocol)?;
    let departures = network(protocol).parse_departures(&respond(200, departure));
    departures.ok()?.into_iter().find_map(|d| d.journey_id)
}

#[test]
fn id_of_a_departure_asks_for_its_run_and_the_run_holds_the_departure() {
    for (protocol, departure, run, identifiers, count) in RUNS {
        let network = network(protocol);
        let departures = network.parse_departures(&respond(200, departure)).unwrap();
        assert!(!departures.is_empty(), "{protocol:?}");

        // Two of the answers hold a second departure, of another run.
        let mut asked: Vec<&Departure> = Vec::new();
        for departure in &departures {
            let id = departure.journey_id.as_deref();
            let id = id.unwrap_or_else(|| panic!("{protocol:?}: {departure:?}"));
            let request = network.journey_request(id).unwrap();
            if carries(protocol, &request, identifiers) {
                asked.push(departure);
            } else if departures.len() == 1 {
                panic!("{protocol:?}: {:?}", sent(protocol, &request));
            }
        }
        assert_eq!(asked.len(), 1, "{protocol:?}: {asked:?}");
        let departure = asked[0];

        let journey = network.parse_journey(&respond(200, run)).unwrap();
        // VRR's OJP server names the line `RE 1 (RRX)` on a departure.
        let line = match protocol {
            Protocol::Ojp1 => "RE1",
            _ => departure.line.name.as_str(),
        };
        assert_eq!(journey.line.name, line, "{protocol:?}");
        assert!(!journey.cancelled, "{protocol:?}");
        assert_eq!(journey.stops.len(), count, "{protocol:?}");
        for stopover in &journey.stops {
            assert_eq!(
                stopover.stop.network.as_deref(),
                Some("test"),
                "{stopover:?}"
            );
        }

        // The order of travel, from a stop the vehicle only leaves to one
        // it only reaches. Two of the runs end after midnight.
        let times: Vec<_> = journey
            .stops
            .iter()
            .map(|stopover| stopover.scheduled_arrival.or(stopover.scheduled_departure))
            .collect();
        assert!(times.iter().all(Option::is_some), "{protocol:?}: {times:?}");
        assert!(times.is_sorted(), "{protocol:?}: {times:?}");
        let first = journey.stops.first().unwrap();
        let last = journey.stops.last().unwrap();
        assert!(
            first.scheduled_departure.is_some() && first.scheduled_arrival.is_none(),
            "{protocol:?}: {first:?}"
        );
        assert!(
            last.scheduled_arrival.is_some() && last.scheduled_departure.is_none(),
            "{protocol:?}: {last:?}"
        );

        let calls: Vec<_> = journey
            .stops
            .iter()
            .filter(|stopover| stopover.stop.id == departure.stop.id)
            .map(|stopover| {
                (
                    stopover.scheduled_departure,
                    stopover.scheduled_platform.as_deref(),
                )
            })
            .collect();
        let expected = (
            Some(departure.scheduled_time),
            departure.scheduled_platform.as_deref(),
        );
        assert!(calls.contains(&expected), "{protocol:?}: {calls:?}");
    }
}

#[test]
fn ride_legs_name_a_run_that_can_be_asked_for_and_walks_name_none() {
    let trips: [(Protocol, &[u8]); 6] = [
        (Protocol::Efa, include_bytes!("fixtures/efa/vvo-trip.json")),
        (
            Protocol::Motis,
            include_bytes!("fixtures/motis/transitous-plan.json"),
        ),
        (
            Protocol::Trias,
            include_bytes!("fixtures/trias/vvo-trip.xml"),
        ),
        (Protocol::Ojp1, include_bytes!("fixtures/ojp/vrr-trip.xml")),
        (
            Protocol::HafasMgate,
            include_bytes!("fixtures/hafas/vbb-trip.json"),
        ),
        (
            Protocol::OtpTransmodel,
            include_bytes!("fixtures/otp/entur-journey-trip.json"),
        ),
    ];
    let mut walks = 0;
    for (protocol, body) in trips {
        let network = network(protocol);
        let found = network.parse_trips(&respond(200, body)).unwrap();
        let mut rides = 0;
        for leg in found.trips.iter().flat_map(|trip| &trip.legs) {
            match leg.kind {
                LegKind::Transit { .. } => {
                    rides += 1;
                    let id = leg.journey_id.as_deref();
                    let id = id.unwrap_or_else(|| panic!("{protocol:?}: {leg:?}"));
                    let request = network.journey_request(id);
                    assert!(request.is_ok(), "{protocol:?}: {request:?}");
                }
                LegKind::Walk => {
                    walks += 1;
                    assert_eq!(leg.journey_id, None, "{protocol:?}: {leg:?}");
                }
                _ => {}
            }
        }
        assert!(rides > 0, "{protocol:?}");
    }
    assert!(walks > 0);
}

#[test]
fn arrivals_name_a_run_that_can_be_asked_for() {
    let arrivals: [(Protocol, &[u8]); 5] = [
        (
            Protocol::Efa,
            include_bytes!("fixtures/efa/vvo-arrivals.json"),
        ),
        (
            Protocol::Motis,
            include_bytes!("fixtures/motis/transitous-arrivals.json"),
        ),
        (
            Protocol::Trias,
            include_bytes!("fixtures/trias/vvo-arrivals.xml"),
        ),
        (
            Protocol::Ojp1,
            include_bytes!("fixtures/ojp/vrr-arrivals.xml"),
        ),
        (
            Protocol::HafasMgate,
            include_bytes!("fixtures/hafas/vbb-arrivals.json"),
        ),
    ];
    // OpenTripPlanner reads arrivals and departures with the same code, and
    // its recorded arrivals are older than the fields that name a run.
    for (protocol, body) in arrivals {
        let network = network(protocol);
        let found = network.parse_arrivals(&respond(200, body)).unwrap();
        assert!(!found.is_empty(), "{protocol:?}");
        for arrival in &found {
            let id = arrival.journey_id.as_deref();
            let id = id.unwrap_or_else(|| panic!("{protocol:?}: {arrival:?}"));
            let request = network.journey_request(id);
            assert!(request.is_ok(), "{protocol:?}: {request:?}");
        }
    }
}

/// What the servers answered for a run they do not know.
#[test]
fn run_the_server_does_not_know_is_not_found() {
    let unknown: [(Protocol, u16, &str); 7] = [
        (
            Protocol::Efa,
            200,
            r#"{"version":"11.0.6.72","leg":{"transportation":{"id":"voe:21066: :H:j26","number":"66","product":{"class":5,"name":"Stadtbus"}},"infos":[]}}"#,
        ),
        (
            Protocol::Motis,
            404,
            r#"{"error":"trip not found: tripId=20261123_03:01_de-DELFI_999999999999"}"#,
        ),
        (
            Protocol::Trias,
            200,
            r#"<?xml version="1.0" encoding="UTF-8"?><trias:Trias xmlns:siri="http://www.siri.org.uk/siri" xmlns:trias="http://www.vdv.de/trias" version="1.2"><trias:ServiceDelivery><siri:ResponseTimestamp>2026-11-23T22:56:11Z</siri:ResponseTimestamp><siri:ProducerRef>DELIVVO2019</siri:ProducerRef><siri:Status>true</siri:Status><trias:Language>de</trias:Language><trias:DeliveryPayload><trias:TripInfoResponse><trias:ErrorMessage><trias:Code>ERROR_INVALID_JOURNEY_REF</trias:Code></trias:ErrorMessage></trias:TripInfoResponse></trias:DeliveryPayload></trias:ServiceDelivery></trias:Trias>"#,
        ),
        (
            Protocol::Ojp1,
            200,
            r#"<?xml version="1.0" encoding="UTF-8"?><siri:OJP xmlns:siri="http://www.siri.org.uk/siri" xmlns:ojp="http://www.vdv.de/ojp" version="1.0"><siri:OJPResponse><siri:ServiceDelivery><siri:ResponseTimestamp>2026-11-23T22:56:10Z</siri:ResponseTimestamp><siri:ProducerRef>EFAController</siri:ProducerRef><siri:Status>true</siri:Status><ojp:OJPTripInfoDelivery><siri:ResponseTimestamp>2026-11-23T22:56:10Z</siri:ResponseTimestamp><siri:Status>false</siri:Status><siri:ErrorCondition><siri:InvalidDataReferencesError number="-100"><siri:InvalidRef>ddb:98X41::R:j26:99999</siri:InvalidRef></siri:InvalidDataReferencesError></siri:ErrorCondition><ojp:CalcTime>10</ojp:CalcTime></ojp:OJPTripInfoDelivery></siri:ServiceDelivery></siri:OJPResponse></siri:OJP>"#,
        ),
        (
            Protocol::HafasMgate,
            200,
            r#"{"ver":"1.33","err":"OK","svcResL":[{"meth":"JourneyDetails","err":"PARAMETER","errTxt":"HCI Service: parameter invalid"}]}"#,
        ),
        (
            Protocol::OtpTransmodel,
            200,
            r#"{"data":{"serviceJourney":null}}"#,
        ),
        // A run that the server knows on another day.
        (
            Protocol::OtpTransmodel,
            200,
            r#"{"data":{"serviceJourney":{"line":{"publicCode":"L1"},"estimatedCalls":[]}}}"#,
        ),
    ];
    for (protocol, status, body) in unknown {
        let journey = network(protocol).parse_journey(&respond(status, body.as_bytes()));
        assert!(
            matches!(journey, Err(Error::NotFound { .. })),
            "{protocol:?}: {journey:?}"
        );
    }
}

/// MOTIS and HAFAS take the server's own ID, so any text goes to the server.
#[test]
fn text_that_is_no_id_is_an_invalid_query_where_the_core_makes_the_id() {
    let text = "not an id";
    for protocol in PROTOCOLS {
        let request = network(protocol).journey_request(text);
        let name = match protocol {
            Protocol::Motis => "tripId",
            Protocol::HafasMgate => "jid",
            _ => {
                assert!(
                    matches!(request, Err(Error::InvalidQuery { .. })),
                    "{protocol:?}: {request:?}"
                );
                continue;
            }
        };
        let request = request.unwrap();
        assert!(
            carries(protocol, &request, &[(name, text)]),
            "{protocol:?}: {request:?}"
        );
    }
}

#[test]
fn network_can_lack_the_operation_and_efa_needs_a_time_zone() {
    for protocol in PROTOCOLS {
        let id = recorded_id(protocol).unwrap();
        let mut without = network(protocol);
        without.unsupported.push(Operation::Journeys);
        let request = without.journey_request(&id);
        assert!(
            matches!(request, Err(Error::Unsupported { .. })),
            "{protocol:?}: {request:?}"
        );
    }

    // The request names the time on the clocks of the network.
    let id = recorded_id(Protocol::Efa).unwrap();
    let request = Network::new("test", "Test", ENDPOINT, Protocol::Efa).journey_request(&id);
    assert!(
        matches!(request, Err(Error::InvalidNetwork { .. })),
        "{request:?}"
    );
}

/// The ID only adds to a departure, so a part of it in a shape that no
/// recorded answer showed must not cost the departure.
#[test]
fn departure_survives_a_name_of_its_run_in_another_shape() {
    let (_, departure, ..) = RUNS[0];
    let mut answer: Value = serde_json::from_slice(departure).unwrap();
    answer["stopEvents"][0]["transportation"]["id"] = serde_json::json!({"odd": true});
    let body = serde_json::to_vec(&answer).unwrap();
    let departures = network(Protocol::Efa).parse_departures(&respond(200, &body));
    let ids: Vec<_> = departures
        .unwrap()
        .into_iter()
        .map(|d| d.journey_id)
        .collect();
    assert_eq!(ids, [None]);
}
