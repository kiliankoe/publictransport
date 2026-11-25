//! Arrivals at a stop, for every protocol: what differs from departures.

use publictransport::{
    Arrival, DeparturesQuery, Error, HafasOptions, HttpRequest, HttpResponse, Network, Protocol,
    Timestamp,
};
use serde_json::Value;

const EFA: &[u8] = include_bytes!("fixtures/efa/vvo-arrivals.json");
const MOTIS: &[u8] = include_bytes!("fixtures/motis/transitous-arrivals.json");
const TRIAS: &[u8] = include_bytes!("fixtures/trias/vvo-arrivals.xml");
const OJP: &[u8] = include_bytes!("fixtures/ojp/vrr-arrivals.xml");
const HAFAS: &[u8] = include_bytes!("fixtures/hafas/vbb-arrivals.json");
const OTP: &[u8] = include_bytes!("fixtures/otp/entur-arrivals.json");

/// A network with what its protocol needs to build a request with a time.
fn network(protocol: Protocol) -> Network {
    let network = Network::new("test", "Test", "https://example.org/base", protocol);
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

/// A time of day on 2026-11-23 in UTC, the day the fixtures were recorded.
fn utc(hour: i64, minute: i64, second: i64) -> Timestamp {
    Timestamp {
        unix_seconds: 1_795_392_000 + hour * 3600 + minute * 60 + second,
    }
}

/// Everything a request sends, as named parts, without the part that tells
/// arrivals from departures. That part's value comes second, if it is sent.
fn split(protocol: Protocol, request: &HttpRequest) -> (Vec<(String, String)>, Option<String>) {
    let (path, query) = request
        .url
        .split_once('?')
        .unwrap_or((request.url.as_str(), ""));
    let body = std::str::from_utf8(request.body.as_deref().unwrap_or_default()).unwrap_or_default();
    let mut parts = vec![
        ("method".to_owned(), format!("{:?}", request.method)),
        ("path".to_owned(), path.to_owned()),
    ];
    parts.extend(
        request
            .headers
            .iter()
            .map(|header| (header.name.clone(), header.value.clone())),
    );
    let mut switch = None;
    let mut keep = |name: &str, value: String, switch_name: &str| {
        if name == switch_name {
            switch = Some(value);
        } else {
            parts.push((name.to_owned(), value));
        }
    };

    match protocol {
        Protocol::Efa | Protocol::Motis => {
            let name = if protocol == Protocol::Efa {
                "itdDateTimeDepArr"
            } else {
                "arriveBy"
            };
            for (key, value) in form_urlencoded::parse(query.as_bytes()) {
                keep(&key, value.into_owned(), name);
            }
        }
        Protocol::Trias | Protocol::Ojp1 => {
            // The text of every element without child elements.
            let document = roxmltree::Document::parse(body);
            let leaves = document.iter().flat_map(roxmltree::Document::descendants);
            for leaf in leaves.filter(|node| {
                node.is_element() && !node.children().any(|child| child.is_element())
            }) {
                let text = leaf.text().unwrap_or_default().to_owned();
                keep(leaf.tag_name().name(), text, "StopEventType");
            }
        }
        Protocol::HafasMgate => {
            let mut body: Value = serde_json::from_str(body).unwrap_or_default();
            let kind = body
                .pointer_mut("/svcReqL/0/req")
                .and_then(Value::as_object_mut)
                .and_then(|req| req.remove("type"));
            if let Some(kind) = kind.as_ref().and_then(Value::as_str) {
                keep("type", kind.to_owned(), "type");
            }
            keep("body", body.to_string(), "type");
        }
        _ => {
            let body: Value = serde_json::from_str(body).unwrap_or_default();
            // GraphQL ignores whitespace and commas.
            let mut graphql: String = body
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_whitespace() && *c != ',')
                .collect();
            let name = "arrivalDeparture:";
            if let Some(start) = graphql.find(name) {
                let value: String = graphql
                    .get(start + name.len()..)
                    .unwrap_or_default()
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect();
                graphql.replace_range(start..start + name.len() + value.len(), "");
                keep(name, value, name);
            }
            keep("query", graphql, name);
            let variables = body.get("variables").cloned().unwrap_or_default();
            keep("variables", variables.to_string(), name);
        }
    }
    (parts, switch)
}

#[test]
fn arrivals_request_is_the_departures_request_with_one_switch() {
    let plain = DeparturesQuery::new("de:14612:28");
    let detailed = plain.clone().with_time(utc(18, 10, 0)).with_limit(7);
    for (protocol, arrival) in [
        (Protocol::Efa, "arr"),
        (Protocol::Motis, "true"),
        (Protocol::Trias, "arrival"),
        (Protocol::Ojp1, "arrival"),
        (Protocol::HafasMgate, "ARR"),
        (Protocol::OtpTransmodel, "arrivals"),
    ] {
        let network = network(protocol);
        for query in [&plain, &detailed] {
            let case = format!("{protocol:?}, {query:?}");
            let departures = split(protocol, &network.departures_request(query).unwrap());
            let arrivals = split(protocol, &network.arrivals_request(query).unwrap());
            assert_eq!(arrivals.0, departures.0, "{case}");
            assert_eq!(arrivals.1.as_deref(), Some(arrival), "{case}");
            assert_ne!(departures.1, arrivals.1, "{case}");
        }
    }
}

/// A server without departures has no arrivals, and bad input is the same
/// bad input.
#[test]
fn arrivals_request_fails_where_the_departures_request_does() {
    let query = DeparturesQuery::new("de:14612:28");
    let timed = query.clone().with_time(utc(18, 10, 0));
    let unsupported = Network::from_json(
        "example",
        r#"{
            "name": "Example",
            "type": { "trias": true },
            "options": {
                "endpoint": "https://example.org/trias",
                "requestorRef": "OpenService",
                "providedServices": ["LocationInformationRequest", "TripRequest"]
            }
        }"#,
    )
    .unwrap();
    assert!(
        matches!(
            unsupported.arrivals_request(&query),
            Err(Error::Unsupported { .. })
        ),
        "{:?}",
        unsupported.arrivals_request(&query)
    );

    let bare = |protocol| Network::new("test", "Test", "https://example.org/base", protocol);
    let never = Timestamp {
        unix_seconds: i64::MAX,
    };
    for (network, query) in [
        (unsupported, &query),
        // The request needs a requestor reference.
        (bare(Protocol::Trias), &query),
        // The request names the time on the clocks of the network.
        (bare(Protocol::Efa), &timed),
        (bare(Protocol::Motis), &query.clone().with_time(never)),
    ] {
        let departures = network.departures_request(query).map(|_| ()).unwrap_err();
        let arrivals = network.arrivals_request(query).map(|_| ()).unwrap_err();
        assert_eq!(
            std::mem::discriminant(&arrivals),
            std::mem::discriminant(&departures),
            "{arrivals:?} instead of {departures:?}"
        );
    }
}

/// Every answer holds an arrival whose times or platforms differ from what a
/// departure board would say, and names its origin next to a destination.
#[test]
fn arrivals_have_the_arrival_time_and_the_origin_and_are_ordered_by_expected_time() {
    for (protocol, body, count, origin, scheduled, realtime) in [
        (
            Protocol::Efa,
            EFA,
            10,
            // The server orders by scheduled time; this tram is early.
            Some("Dresden Striesen Ludwig-Hartmann-Straße"),
            utc(18, 15, 0),
            Some(utc(18, 13, 42)),
        ),
        (
            Protocol::Motis,
            MOTIS,
            // One of them ends here and has no departure.
            13,
            // Departs at 18:10.
            Some("Dresden Flughafen"),
            utc(18, 8, 0),
            Some(utc(18, 8, 0)),
        ),
        (
            Protocol::Trias,
            TRIAS,
            10,
            Some("Dresden Striesen Ludwig-Hartmann-Straße"),
            utc(18, 15, 0),
            Some(utc(18, 13, 42)),
        ),
        (
            Protocol::Ojp1,
            OJP,
            10,
            Some("D-Theodor-Heuss-Brücke"),
            utc(18, 8, 0),
            Some(utc(18, 12, 48)),
        ),
        (
            Protocol::HafasMgate,
            HAFAS,
            10,
            // The times are local to Berlin. The server lists this early
            // train after later ones.
            Some("S+U Wittenau (Berlin)"),
            utc(18, 13, 0),
            Some(utc(18, 12, 0)),
        ),
        (
            Protocol::OtpTransmodel,
            OTP,
            10,
            // The answer does not name the origin. The train departs at
            // 18:16, and the server orders by departure.
            None,
            utc(18, 13, 0),
            Some(utc(18, 12, 22)),
        ),
    ] {
        let response = HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.to_vec(),
        };
        let arrivals = network(protocol)
            .parse_arrivals(&publictransport::DeparturesQuery::new("stop"), &response)
            .unwrap();
        let found: Vec<_> = arrivals
            .iter()
            .map(|a| (a.origin.as_deref(), a.scheduled_time, a.realtime_time))
            .collect();
        assert_eq!(found.len(), count, "{protocol:?}: {found:?}");
        assert!(
            found.contains(&(origin, scheduled, realtime)),
            "{protocol:?}: {found:?}"
        );
        assert!(
            arrivals.is_sorted_by_key(Arrival::expected_time),
            "{protocol:?}: {found:?}"
        );
    }
}

#[test]
fn hafas_reads_the_platforms_of_the_arrival() {
    let response = HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: HAFAS.to_vec(),
    };
    let arrivals = network(Protocol::HafasMgate)
        .parse_arrivals(&publictransport::DeparturesQuery::new("stop"), &response)
        .unwrap();
    let platforms = |origin: &str| {
        let arrival = arrivals
            .iter()
            .find(|a| a.origin.as_deref() == Some(origin));
        arrival.map(|a| {
            (
                a.scheduled_platform.as_deref(),
                a.realtime_platform.as_deref(),
            )
        })
    };
    assert_eq!(platforms("S Erkner Bhf"), Some((Some("4"), Some("4"))));
    assert_eq!(
        platforms("S+U Warschauer Str. (Berlin)"),
        Some((Some("Pos. 17"), None))
    );

    // A stop that the vehicle also leaves from has a platform and a
    // cancellation for each side.
    let mut both: serde_json::Value = serde_json::from_slice(HAFAS).unwrap();
    let stop = &mut both["svcResL"][0]["res"]["jnyL"][0]["stbStop"];
    stop["dTimeS"] = "191500".into();
    stop["dPltfS"] = serde_json::json!({"type": "PL", "txt": "9"});
    stop["dCncl"] = true.into();
    let response = HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: both.to_string().into_bytes(),
    };
    let arrivals = network(Protocol::HafasMgate)
        .parse_arrivals(&publictransport::DeparturesQuery::new("stop"), &response)
        .unwrap();
    let arrival = arrivals
        .iter()
        .find(|a| a.origin.as_deref() == Some("S+U Warschauer Str. (Berlin)"))
        .unwrap();
    assert_eq!(
        (arrival.platform(), arrival.cancelled),
        (Some("Pos. 17"), false)
    );
}
