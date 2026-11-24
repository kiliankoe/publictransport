//! The language of a network: how each protocol asks for it, and how the
//! texts of an `OpenTripPlanner` server are picked by it.

use publictransport::{
    Coordinate, DeparturesQuery, Error, HafasOptions, HttpRequest, HttpResponse, LocationQuery,
    NearbyQuery, Network, Place, Protocol, TripsQuery,
};
use serde_json::{Value, json};

const ENDPOINT: &str = "https://example.org/base";
const GEOCODER: &str = "https://example.org/geocoder";

/// No code that a server knows: it shows a change of case, and a text that
/// went into a URL or into XML without its encoding.
const LANGUAGE: &str = "pt-BR &<é";

/// A protocol with a recorded answer that holds a departure, whose run the
/// request for a journey asks for.
const PROTOCOLS: [(Protocol, &[u8]); 6] = [
    (
        Protocol::Efa,
        include_bytes!("fixtures/efa/vvo-journey-departure.json"),
    ),
    (
        Protocol::Motis,
        include_bytes!("fixtures/motis/transitous-journey-departure.json"),
    ),
    (
        Protocol::Trias,
        include_bytes!("fixtures/trias/vvo-journey-departure.xml"),
    ),
    (
        Protocol::Ojp1,
        include_bytes!("fixtures/ojp/vrr-journey-departure.xml"),
    ),
    (
        Protocol::HafasMgate,
        include_bytes!("fixtures/hafas/vbb-journey-departure.json"),
    ),
    (
        Protocol::OtpTransmodel,
        include_bytes!("fixtures/otp/entur-journey-departure.json"),
    ),
];

/// A network with what its protocol needs to build every kind of request.
fn network(protocol: Protocol) -> Network {
    let network = Network::new("test", "Test", ENDPOINT, protocol);
    match protocol {
        Protocol::Efa => network.with_timezone("Europe/Berlin"),
        Protocol::Trias | Protocol::Ojp1 => network.with_requestor_ref("OpenService"),
        Protocol::HafasMgate => network
            .with_hafas(
                HafasOptions::new(
                    r#"{"type":"WEB","id":"VBB","name":"VBB WebApp","l":"vs_webapp_vbb"}"#,
                    r#"{"type":"AID","aid":"hafas-vbb-webapp"}"#,
                    "1.33",
                )
                .with_language("de"),
            )
            .with_timezone("Europe/Berlin"),
        Protocol::OtpTransmodel => network.with_geocoder(GEOCODER),
        _ => network,
    }
}

fn respond(body: impl AsRef<[u8]>) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.as_ref().to_vec(),
    }
}

/// One request of every kind, with the name of the kind.
fn requests(
    network: &Network,
    departure: &[u8],
) -> Vec<(&'static str, Result<HttpRequest, Error>)> {
    let stop = || Place::Id("de:14612:28".into());
    let departures = network.parse_departures(&respond(departure));
    let mut runs = departures
        .into_iter()
        .flatten()
        .filter_map(|d| d.journey_id);
    // Without a run, the request for it fails.
    let run = runs.next().unwrap_or_default();
    let position = Coordinate {
        latitude: 51.04,
        longitude: 13.7325,
    };
    let stop_events = DeparturesQuery::new("de:14612:28");
    vec![
        (
            "locations",
            network.locations_request(&LocationQuery::new("Hauptbahnhof")),
        ),
        (
            "nearby",
            network.nearby_request(&NearbyQuery::new(position)),
        ),
        ("departures", network.departures_request(&stop_events)),
        ("arrivals", network.arrivals_request(&stop_events)),
        (
            "trips",
            network.trips_request(&TripsQuery::new(stop(), stop())),
        ),
        ("journey", network.journey_request(&run)),
    ]
}

/// The body as JSON, `null` if it is none.
fn body(request: &HttpRequest) -> Value {
    serde_json::from_slice(request.body.as_deref().unwrap_or_default()).unwrap_or_default()
}

/// The decoded values of every query parameter called `name`.
fn parameter(request: &HttpRequest, name: &str) -> Vec<String> {
    let query = request.url.split_once('?').map_or("", |(_, query)| query);
    form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// The language that an XML body names. The check of the requests against
/// the schemas covers where its element belongs.
fn xml_language(request: &HttpRequest) -> Result<Option<String>, String> {
    let body = std::str::from_utf8(request.body.as_deref().unwrap_or_default());
    let body = body.map_err(|error| error.to_string())?;
    let document = roxmltree::Document::parse(body).map_err(|error| format!("{error}: {body}"))?;
    let mut languages = document
        .descendants()
        .filter(|node| node.tag_name().name() == "Language")
        .map(|node| node.text().unwrap_or_default().to_owned());
    match (languages.next(), languages.next()) {
        (language, None) => Ok(language),
        _ => Err(format!("more than one: {body}")),
    }
}

/// The language a request asks in, by the way of its protocol: `None` if it
/// names none, an error if it does not name it as the protocol does.
fn asked(protocol: Protocol, request: &HttpRequest) -> Result<Option<String>, String> {
    let only = |mut values: Vec<String>| match values.len() {
        0 | 1 => Ok(values.pop()),
        _ => Err(format!("more than one: {values:?}")),
    };
    match protocol {
        Protocol::Efa | Protocol::Motis => only(parameter(request, "language")),
        Protocol::Trias | Protocol::Ojp1 => xml_language(request),
        Protocol::HafasMgate => {
            let body = body(request);
            Ok(body.get("lang").and_then(Value::as_str).map(str::to_owned))
        }
        // Only the geocoder takes a language. The other answers come in
        // every language.
        _ => only(parameter(request, "lang")),
    }
}

/// MOTIS has no parameter for the language of the places around a position,
/// and an `OpenTripPlanner` server is asked for nothing but locations in one
/// language. HAFAS has to name a language and takes the one of its options.
#[test]
fn every_request_names_the_language_as_its_protocol_does_and_none_without_one() {
    for (protocol, departure) in PROTOCOLS {
        let plain = network(protocol);
        let default = (protocol == Protocol::HafasMgate).then(|| "de".to_owned());
        for (kind, request) in requests(&plain, departure) {
            let asked = asked(protocol, &request.unwrap());
            assert_eq!(asked, Ok(default.clone()), "{protocol:?} {kind}");
        }

        // A setting that an application left empty.
        let blank = plain.clone().with_language(" ");
        for (kind, request) in requests(&blank, departure) {
            let asked = asked(protocol, &request.unwrap());
            assert_eq!(asked, Ok(default.clone()), "{protocol:?} {kind}");
        }

        let network = plain.with_language(LANGUAGE);
        for (kind, request) in requests(&network, departure) {
            let expected = match (protocol, kind) {
                (Protocol::Motis, "nearby") => None,
                (Protocol::OtpTransmodel, kind) if kind != "locations" => None,
                _ => Some(LANGUAGE.to_owned()),
            };
            let asked = asked(protocol, &request.unwrap());
            assert_eq!(asked, Ok(expected), "{protocol:?} {kind}");
        }
    }
}

/// The server sends a text once per language, so only the name of the
/// language next to it tells which one to take.
#[test]
fn otp_queries_ask_for_the_language_of_each_disruption_text() {
    let network = network(Protocol::OtpTransmodel);
    let stop = || Place::Id("NSR:StopPlace:337".into());
    let requests = [
        network.departures_request(&DeparturesQuery::new("NSR:StopPlace:337")),
        network.trips_request(&TripsQuery::new(stop(), stop())),
    ];
    for request in requests {
        let query = body(&request.unwrap())["query"].to_string();
        for text in ["summary{value language}", "description{value language}"] {
            assert!(query.contains(text), "{query}");
        }
    }
}

/// A text of a disruption: the language it names, if any, and what it says.
type Text<'a> = (Option<&'a str>, &'a str);

/// A departure of an `OpenTripPlanner` server with one disruption. Its
/// summary and its description hold the given texts, each with its language
/// if it names one.
fn disrupted(texts: &[Text]) -> HttpResponse {
    let entries = |prefix: &str| -> Vec<Value> {
        texts
            .iter()
            .map(|(language, text)| {
                let value = if text.is_empty() {
                    String::new()
                } else {
                    format!("{prefix} {text}")
                };
                match language {
                    Some(language) => json!({"value": value, "language": language}),
                    None => json!({"value": value}),
                }
            })
            .collect()
    };
    let time = "2026-11-16T14:24:00+01:00";
    let call = json!({
        "aimedArrivalTime": time,
        "expectedArrivalTime": time,
        "aimedDepartureTime": time,
        "expectedDepartureTime": time,
        "realtime": true,
        "cancellation": false,
        "destinationDisplay": {"frontText": "Somewhere"},
        "quay": {"id": "NSR:Quay:555", "name": "Tøyen", "latitude": 59.5, "longitude": 10.75},
        "serviceJourney": {"line": {
            "publicCode": "5",
            "name": "Here - There",
            "transportMode": "metro",
            "transportSubmode": "metro",
            "authority": {"name": "Operator"}
        }},
        "situations": [{
            "situationNumber": "RUT:SituationNumber:823910",
            "summary": entries("Title"),
            "description": entries("Text"),
            "infoLinks": [],
            "validityPeriods": []
        }],
        "notices": []
    });
    respond(json!({"data": {"quay": {"estimatedCalls": [call]}}}).to_string())
}

#[test]
fn otp_remark_takes_the_text_in_the_language_of_the_network_or_else_the_first_one() {
    let both = [(Some("no"), "norsk"), (Some("en"), "english")];
    let cases: [(Option<&str>, &[Text], &str); 6] = [
        (Some("en"), &both, "english"),
        (Some("EN"), &both, "english"),
        (Some("de"), &both, "norsk"),
        (None, &both, "norsk"),
        // Entries that name no language match none.
        (
            Some("en"),
            &[(None, "norsk"), (Some("en"), "english")],
            "english",
        ),
        (
            Some("de"),
            &[(Some("no"), ""), (Some("en"), "english")],
            "english",
        ),
    ];
    for (language, texts, expected) in cases {
        let mut network = network(Protocol::OtpTransmodel);
        network.language = language.map(str::to_owned);
        let departures = network.parse_departures(&disrupted(texts)).unwrap();
        let remarks: Vec<_> = departures
            .iter()
            .flat_map(|departure| &departure.remarks)
            .map(|remark| (remark.title.clone(), remark.text.clone()))
            .collect();
        let expected = (
            Some(format!("Title {expected}")),
            Some(format!("Text {expected}")),
        );
        assert_eq!(remarks, [expected], "{language:?}: {texts:?}");
    }
}
