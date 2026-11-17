//! The client against a mock server on localhost.

use publictransport_client::reqwest::{self, Url};
use publictransport_client::{
    ApiError, Client, DeparturesQuery, Error, Location, LocationQuery, Network, Place, Protocol,
    Timestamp, TripsQuery,
};
use publictransport_client::{HttpResponse, Method};
use wiremock::matchers::{any, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const EFA_LOCATIONS: &[u8] = include_bytes!("../../core/tests/fixtures/efa/vvo-stopfinder.json");
const EFA_DEPARTURES: &[u8] = include_bytes!("../../core/tests/fixtures/efa/vvo-departures.json");
const EFA_TRIPS: &[u8] = include_bytes!("../../core/tests/fixtures/efa/vvo-trip.json");
const MOTIS_LOCATIONS: &[u8] =
    include_bytes!("../../core/tests/fixtures/motis/transitous-geocode.json");
const MOTIS_DEPARTURES: &[u8] =
    include_bytes!("../../core/tests/fixtures/motis/transitous-stoptimes.json");
const MOTIS_TRIPS: &[u8] = include_bytes!("../../core/tests/fixtures/motis/transitous-plan.json");

const USER_AGENT: &str = "publictransport-tests/1.0 (+https://example.org)";

/// 2026-11-16T13:24:00Z, the day the fixtures were recorded.
const TIME: Timestamp = Timestamp {
    unix_seconds: 1_794_835_440,
};

fn efa(endpoint: impl Into<String>) -> Network {
    Network::new("test", "Test", endpoint, Protocol::Efa)
}

/// Searches for locations on an EFA server that answers every request with
/// `response`.
async fn locations_from(response: ResponseTemplate) -> Result<Vec<Location>, Error> {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(response)
        .mount(&server)
        .await;
    Client::new(USER_AGENT)?
        .locations(&efa(server.uri()), &LocationQuery::new("Hauptbahnhof"))
        .await
}

#[tokio::test]
async fn each_operation_sends_the_cores_request_and_returns_what_the_core_parses() {
    let client = Client::new(USER_AGENT).unwrap();
    for (protocol, prefix, stops, [locations, departures, trips]) in [
        (
            Protocol::Efa,
            "/std3",
            ["de:14612:28", "de:14612:13"],
            [
                ("/std3/XML_STOPFINDER_REQUEST", EFA_LOCATIONS),
                ("/std3/XML_DM_REQUEST", EFA_DEPARTURES),
                ("/std3/XML_TRIP_REQUEST2", EFA_TRIPS),
            ],
        ),
        (
            Protocol::Motis,
            "/api",
            ["de-DELFI_de:14612:28", "de-DELFI_de:14612:13"],
            [
                ("/api/v1/geocode", MOTIS_LOCATIONS),
                ("/api/v6/stoptimes", MOTIS_DEPARTURES),
                ("/api/v6/plan", MOTIS_TRIPS),
            ],
        ),
    ] {
        let server = MockServer::start().await;
        for (route, body) in [locations, departures, trips] {
            Mock::given(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&server)
                .await;
        }
        let network = Network::new("test", "Test", server.uri() + prefix, protocol)
            .with_timezone("Europe/Berlin");
        let ok = |body: &[u8]| HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.to_vec(),
        };

        let locations_query = LocationQuery::new("Dresden Hbf &=#/ä").with_limit(5);
        let expected = network.parse_locations(&ok(locations.1)).unwrap();
        assert_ne!(expected, [], "the fixture has results");
        let found = client.locations(&network, &locations_query).await.unwrap();
        assert_eq!(found, expected);

        let departures_query = DeparturesQuery::new(stops[0]).with_time(TIME);
        let expected = network.parse_departures(&ok(departures.1)).unwrap();
        assert_ne!(expected, [], "the fixture has results");
        let found = client
            .departures(&network, &departures_query)
            .await
            .unwrap();
        assert_eq!(found, expected);

        let [from, to] = stops.map(|stop| Place::Id(stop.to_owned()));
        let trips_query = TripsQuery::new(from, to).with_departure(TIME);
        let expected = network.parse_trips(&ok(trips.1)).unwrap();
        assert_ne!(expected.trips, [], "the fixture has results");
        let found = client.trips(&network, &trips_query).await.unwrap();
        assert_eq!(found, expected);

        let built = [
            network.locations_request(&locations_query).unwrap(),
            network.departures_request(&departures_query).unwrap(),
            network.trips_request(&trips_query).unwrap(),
        ];
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), built.len());
        let parameters = |url: &Url| url.query_pairs().into_owned().collect::<Vec<_>>();
        for (received, built) in received.iter().zip(&built) {
            let url = Url::parse(&built.url).unwrap();
            let method = match built.method {
                Method::Get => "GET",
                Method::Post => "POST",
            };
            assert_eq!(received.method.as_str(), method);
            assert_eq!(received.url.path(), url.path());
            assert_eq!(parameters(&received.url), parameters(&url));
            assert_eq!(received.headers.get("user-agent").unwrap(), USER_AGENT);
        }
    }
}

#[tokio::test]
async fn redirect_is_reported_as_moved_and_not_followed() {
    for status in [301, 302] {
        let server = MockServer::start().await;
        let target = server.uri() + "/new";
        // A client that follows the redirect gets a valid answer here.
        Mock::given(path("/new"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(EFA_LOCATIONS))
            .mount(&server)
            .await;
        Mock::given(path("/old/XML_STOPFINDER_REQUEST"))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", target.as_str()))
            .mount(&server)
            .await;

        let result = Client::new(USER_AGENT)
            .unwrap()
            .locations(
                &efa(server.uri() + "/old"),
                &LocationQuery::new("Hauptbahnhof"),
            )
            .await;
        assert!(
            matches!(
                &result,
                Err(Error::Api(ApiError::Moved { location })) if location.as_ref() == Some(&target)
            ),
            "{status}: {result:?}"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn failure_status_and_body_reach_the_core() {
    let result = locations_from(ResponseTemplate::new(418).set_body_string("teapot")).await;
    assert!(
        matches!(
            &result,
            Err(Error::Api(ApiError::Server { status: 418, message })) if message == "teapot"
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn oversized_response_is_cut_off() {
    let response = ResponseTemplate::new(200).set_body_bytes(vec![b' '; 17 * 1024 * 1024]);
    let result = locations_from(response).await;
    assert!(matches!(result, Err(Error::TooLarge)), "{result:?}");
}

#[tokio::test]
async fn query_the_core_rejects_sends_no_request() {
    let server = MockServer::start().await;
    let result = Client::new(USER_AGENT)
        .unwrap()
        .departures(
            &efa(server.uri()),
            &DeparturesQuery::new("de:14612:28").with_time(TIME),
        )
        .await;
    assert!(
        matches!(result, Err(Error::Api(ApiError::InvalidNetwork { .. }))),
        "{result:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn request_without_an_answer_is_a_transport_error() {
    // The port is free again once the listener is dropped.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let result = Client::new(USER_AGENT)
        .unwrap()
        .locations(
            &efa(format!("http://127.0.0.1:{port}")),
            &LocationQuery::new("Hauptbahnhof"),
        )
        .await;
    assert!(matches!(result, Err(Error::Transport(_))), "{result:?}");
}

#[test]
fn user_agent_that_is_no_header_value_is_a_transport_error() {
    let result = Client::new("line\nbreak");
    assert!(matches!(result, Err(Error::Transport(_))), "{result:?}");
}

#[tokio::test]
async fn caller_configured_http_client_is_used_as_it_is() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_bytes(EFA_LOCATIONS))
        .mount(&server)
        .await;
    let http = reqwest::Client::builder()
        .user_agent("configured-by-the-caller/2.0")
        .build()
        .unwrap();
    Client::with_http_client(http)
        .locations(&efa(server.uri()), &LocationQuery::new("Hauptbahnhof"))
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(
        received[0].headers.get("user-agent").unwrap(),
        "configured-by-the-caller/2.0"
    );
}

/// TRIAS is the protocol whose requests have a body and a header of their
/// own, next to the one of the network.
#[tokio::test]
async fn request_body_and_headers_arrive_as_the_core_built_them() {
    let fixture = include_bytes!("../../core/tests/fixtures/trias/vvo-locations.xml");
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture.as_slice()))
        .mount(&server)
        .await;
    let network = Network::new("test", "Test", server.uri() + "/trias", Protocol::Trias)
        .with_requestor_ref("OpenService")
        .with_header("X-Api-Key", "secret");
    let query = LocationQuery::new("Hauptbahnhof");

    let found = Client::new(USER_AGENT)
        .unwrap()
        .locations(&network, &query)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    let built = network.locations_request(&query).unwrap();
    let received = server.received_requests().await.unwrap();
    let [received] = received.as_slice() else {
        panic!("{received:?}");
    };
    assert_eq!(received.method.as_str(), "POST");
    assert_eq!(Some(&received.body), built.body.as_ref());
    for header in &built.headers {
        let value = received.headers.get(header.name.as_str()).unwrap();
        assert_eq!(value.to_str().unwrap(), header.value);
    }
}
