//! The registry of networks: reading entries, coverage and the bundled set.

use proptest::prelude::*;
use publictransport::{
    Area, Coordinate, Coverage, CoverageTier, DeparturesQuery, Error, HafasProduct, LocationQuery,
    Network, Operation, Place, Polygon, Protocol, Registry, TripsQuery,
};

/// The TRIAS and OJP service behind each operation.
const SERVICES: [(Operation, &str); 3] = [
    (Operation::Locations, "LocationInformationRequest"),
    (Operation::Departures, "StopEventRequest"),
    (Operation::Trips, "TripRequest"),
];

fn at(latitude: f64, longitude: f64) -> Coordinate {
    Coordinate {
        latitude,
        longitude,
    }
}

/// A closed ring around the rectangle between two corners.
fn ring(south: f64, west: f64, north: f64, east: f64) -> Vec<Coordinate> {
    vec![
        at(south, west),
        at(south, east),
        at(north, east),
        at(north, west),
        at(south, west),
    ]
}

fn area(regions: &[&str], polygons: Vec<Polygon>) -> Area {
    let mut area = Area::default();
    area.regions = regions.iter().map(ToString::to_string).collect();
    area.polygons = polygons;
    area
}

fn ids(networks: &[Network]) -> Vec<&str> {
    networks.iter().map(|network| network.id.as_str()).collect()
}

/// Whether building the request for an operation is refused as unsupported.
fn refuses(network: &Network, operation: Operation) -> bool {
    let result = match operation {
        Operation::Locations => network.locations_request(&LocationQuery::new("Hauptbahnhof")),
        Operation::Departures => network.departures_request(&DeparturesQuery::new("de:14612:28")),
        Operation::Trips => network.trips_request(&TripsQuery::new(
            Place::Id("de:14612:28".into()),
            Place::Id("de:14612:13".into()),
        )),
        _ => unreachable!("an operation this test does not know"),
    };
    match result {
        Ok(_) => false,
        Err(Error::Unsupported { .. }) => true,
        Err(error) => unreachable!("{operation:?}: {error:?}"),
    }
}

#[test]
fn reads_an_entry() {
    let network = Network::from_json(
        "xx-efa",
        r#"{
            "name": "Example",
            "type": { "efa": true },
            "timezone": "Europe/Berlin",
            "supportedLanguages": ["de"],
            "attribution": {
                "name": "Example Transit",
                "homepage": "https://example.org/",
                "license": "CC-BY-4.0",
                "mixedLicenses": false
            },
            "coverage": {
                "realtimeCoverage": {
                    "region": ["DE-SN", "CZ"],
                    "area": {
                        "type": "Polygon",
                        "coordinates": [
                            [[10, 50], [20, 50], [20, 60], [10, 60], [10, 50]],
                            [[12, 52], [12, 58], [18, 58], [18, 52], [12, 52]]
                        ]
                    }
                },
                "regularCoverage": {
                    "area": {
                        "type": "MultiPolygon",
                        "coordinates": [
                            [[[1, 2], [3, 2], [3, 4], [1, 2]]],
                            [[[5, 6], [7, 6], [7, 8], [5, 6]]]
                        ]
                    }
                },
                "anyCoverage": { "region": ["DE"] }
            },
            "options": {
                "endpoint": "https://example.org/efa/",
                "supportedOutputFormats": ["XML", "JSON"],
                "xmlOutputFormat": "full"
            }
        }"#,
    )
    .unwrap();

    assert_eq!(network.id, "xx-efa");
    assert_eq!(network.name, "Example");
    assert_eq!(network.protocol, Protocol::Efa);
    assert_eq!(network.endpoint, "https://example.org/efa/");
    assert_eq!(network.timezone.as_deref(), Some("Europe/Berlin"));
    assert_eq!(network.requestor_ref, None);
    assert_eq!(network.unsupported, Vec::new());
    assert!(!network.requires_key);
    assert_eq!(network.terms, None);
    assert_eq!(network.hafas, None);

    let attribution = network.attribution.expect("an attribution");
    assert_eq!(attribution.name, "Example Transit");
    assert_eq!(
        attribution.homepage.as_deref(),
        Some("https://example.org/")
    );
    assert_eq!(attribution.license.as_deref(), Some("CC-BY-4.0"));

    let triangle = |south, west| Polygon {
        rings: vec![vec![
            at(south, west),
            at(south, west + 2.0),
            at(south + 2.0, west + 2.0),
            at(south, west),
        ]],
    };
    let mut coverage = Coverage::default();
    coverage.realtime = Some(area(
        &["DE-SN", "CZ"],
        vec![Polygon {
            rings: vec![
                ring(50.0, 10.0, 60.0, 20.0),
                vec![
                    at(52.0, 12.0),
                    at(58.0, 12.0),
                    at(58.0, 18.0),
                    at(52.0, 18.0),
                    at(52.0, 12.0),
                ],
            ],
        }],
    ));
    coverage.regular = Some(area(&[], vec![triangle(2.0, 1.0), triangle(6.0, 5.0)]));
    coverage.any = Some(area(&["DE"], Vec::new()));
    assert_eq!(network.coverage, coverage);
}

#[test]
fn reads_the_protocol_and_its_endpoint() {
    let cases = [
        ("trias", "", Protocol::Trias, "https://example.org/"),
        (
            "ojp",
            r#", "ojpVersion": "1.0""#,
            Protocol::Ojp1,
            "https://example.org/",
        ),
        ("motis", "", Protocol::Motis, "https://example.org/api"),
    ];
    for (key, option, protocol, endpoint) in cases {
        let json = format!(
            r#"{{
                "name": "Example",
                "type": {{ "{key}": true }},
                "options": {{ "endpoint": "https://example.org/"{option} }}
            }}"#
        );
        let network = Network::from_json("example", &json).unwrap();
        assert_eq!(network.protocol, protocol, "{key}");
        assert_eq!(network.endpoint, endpoint, "{key}");
    }
}

#[test]
fn rejects_entries_it_cannot_serve() {
    let entry = |kind: &str, options: &str| {
        format!(
            r#"{{
                "name": "Example",
                "type": {{ "{kind}": true }},
                "options": {{ "endpoint": "https://example.org/"{options} }}
            }}"#
        )
    };
    let cases = [
        ("no JSON", "<html></html>".to_owned()),
        ("no entry", "[1, 2]".to_owned()),
        ("OTP", entry("otpGraphQl", "")),
        (
            "TRIAS with a header for the key",
            entry(
                "trias",
                r#", "authorizationMethod": "Authorization-Header""#,
            ),
        ),
        ("OJP 2.0", entry("ojp", r#", "ojpVersion": "2.0""#)),
        (
            "HAFAS with signed requests",
            entry(
                "hafasMgate",
                r#", "client": {}, "auth": {}, "ver": "1.44", "checksumSalt": "abc""#,
            ),
        ),
    ];
    for (case, json) in cases {
        let result = Network::from_json("example", &json);
        assert!(
            matches!(result, Err(Error::InvalidNetwork { .. })),
            "{case}: {result:?}"
        );
    }
}

/// Entries name the version `ver` or `version`, and the language is the
/// first one they support.
#[test]
fn reads_the_hafas_options() {
    let cases = [
        ("ver", r#""supportedLanguages": ["de", "en"],"#, "de"),
        ("version", "", "en"),
    ];
    for (version_key, languages, language) in cases {
        let json = format!(
            r#"{{
                "name": "Example",
                "type": {{ "hafasMgate": true }},
                {languages}
                "timezone": "Europe/Berlin",
                "options": {{
                    "endpoint": "https://example.org/bin/mgate.exe",
                    "client": {{ "type": "WEB", "id": "EXAMPLE" }},
                    "auth": {{ "type": "AID", "aid": "secret" }},
                    "ext": "EX.1",
                    "{version_key}": "1.33",
                    "products": [{{ "id": "bus", "bitmasks": [8, 16], "name": "Bus" }}]
                }}
            }}"#
        );
        let network = Network::from_json("example", &json).unwrap();
        assert_eq!(network.protocol, Protocol::HafasMgate);
        assert_eq!(network.endpoint, "https://example.org/bin/mgate.exe");
        assert_eq!(network.timezone.as_deref(), Some("Europe/Berlin"));

        let options = network.hafas.expect("HAFAS options");
        let object = |text: &str| serde_json::from_str::<serde_json::Value>(text).unwrap();
        assert_eq!(
            object(&options.client),
            serde_json::json!({ "type": "WEB", "id": "EXAMPLE" })
        );
        assert_eq!(
            object(&options.auth),
            serde_json::json!({ "type": "AID", "aid": "secret" })
        );
        assert_eq!(options.ext.as_deref(), Some("EX.1"));
        assert_eq!(options.version, "1.33", "{version_key}");
        assert_eq!(options.language, language);
        assert_eq!(
            options.products,
            [HafasProduct {
                id: "bus".to_owned(),
                name: "Bus".to_owned(),
                bitmasks: vec![8, 16],
            }]
        );
    }
}

#[test]
fn reads_the_key_options() {
    let network = Network::from_json(
        "example",
        r#"{
            "name": "Example",
            "type": { "trias": true },
            "options": {
                "endpoint": "https://example.org/trias",
                "requestorRef": "OpenService",
                "requiresSignUp": true
            }
        }"#,
    )
    .unwrap();
    assert_eq!(network.requestor_ref.as_deref(), Some("OpenService"));
    assert!(network.requires_key);
    assert_eq!(network.unsupported, Vec::new());
}

#[test]
fn reads_the_usage_terms() {
    let network = Network::from_json(
        "example",
        r#"{
            "name": "Example",
            "type": { "motis": true },
            "usageTerms": "https://example.org/terms",
            "options": { "endpoint": "https://example.org/" }
        }"#,
    )
    .unwrap();
    assert_eq!(network.terms.as_deref(), Some("https://example.org/terms"));
}

/// For every choice of provided services: the missing ones are unsupported
/// and refused, the others still build a request.
#[test]
fn refuses_operations_the_server_does_not_provide() {
    for (kind, version) in [("trias", ""), ("ojp", r#""ojpVersion": "1.0","#)] {
        for provided in 0..8 {
            let is_provided = |index: usize| provided & (1 << index) != 0;
            let services: Vec<String> = SERVICES
                .iter()
                .enumerate()
                .filter(|(index, _)| is_provided(*index))
                .map(|(_, (_, service))| format!(r#""{service}""#))
                .collect();
            let json = format!(
                r#"{{
                    "name": "Example",
                    "type": {{ "{kind}": true }},
                    "options": {{
                        "endpoint": "https://example.org/trias",
                        {version}
                        "requestorRef": "OpenService",
                        "providedServices": [{}]
                    }}
                }}"#,
                services.join(", ")
            );
            let network = Network::from_json("example", &json).unwrap();
            for (index, (operation, service)) in SERVICES.iter().enumerate() {
                let case = format!("{kind} providing {services:?}, {service}");
                assert_eq!(
                    network.unsupported.contains(operation),
                    !is_provided(index),
                    "{case}"
                );
                assert_eq!(refuses(&network, *operation), !is_provided(index), "{case}");
            }
        }
    }
}

/// Four different whole degrees in ascending order: the outline and the hole
/// of a rectangle along one axis.
fn bounds() -> impl Strategy<Value = Vec<f64>> {
    prop::collection::btree_set(-20i32..=20, 4)
        .prop_map(|set| set.into_iter().map(f64::from).collect())
}

/// Half degrees, so that a position never lies on an edge made of `bounds`.
fn off_edge() -> impl Strategy<Value = f64> {
    (-22i32..22).prop_map(|degrees| f64::from(degrees) + 0.5)
}

proptest! {
    #[test]
    fn tier_at_matches_a_rectangle_with_a_hole(
        latitudes in bounds(),
        longitudes in bounds(),
        latitude in off_edge(),
        longitude in off_edge(),
    ) {
        let within = |outer: bool| {
            let (low, high) = if outer { (0, 3) } else { (1, 2) };
            (latitudes[low]..latitudes[high]).contains(&latitude)
                && (longitudes[low]..longitudes[high]).contains(&longitude)
        };
        let mut hole = ring(latitudes[1], longitudes[1], latitudes[2], longitudes[2]);
        hole.reverse();
        let polygon = Polygon {
            rings: vec![ring(latitudes[0], longitudes[0], latitudes[3], longitudes[3]), hole],
        };
        let mut coverage = Coverage::default();
        coverage.any = Some(area(&[], vec![polygon]));

        let expected = (within(true) && !within(false)).then_some(CoverageTier::Any);
        prop_assert_eq!(coverage.tier_at(at(latitude, longitude)), expected);
    }

    /// Each network covers the position in a random set of tiers. In the other
    /// tiers it names a region and a polygon elsewhere, which do not count.
    #[test]
    fn covering_orders_by_the_best_tier(
        tiers in prop::collection::vec(prop::array::uniform3(any::<bool>()), 0..7),
    ) {
        let position = || at(5.0, 5.0);
        let tier_area = |covers: bool| {
            let south = if covers { 0.0 } else { 20.0 };
            let polygon = Polygon { rings: vec![ring(south, 0.0, south + 10.0, 10.0)] };
            Some(area(&["DE"], vec![polygon]))
        };
        let best = |covers: &[bool; 3]| {
            [CoverageTier::Realtime, CoverageTier::Regular, CoverageTier::Any]
                .into_iter()
                .zip(covers)
                .find_map(|(tier, covers)| covers.then_some(tier))
        };
        let networks: Vec<Network> = tiers
            .iter()
            .enumerate()
            .map(|(index, covers)| {
                let mut network =
                    Network::new(index.to_string(), "Test", "https://example.org", Protocol::Efa);
                network.coverage.realtime = tier_area(covers[0]);
                network.coverage.regular = tier_area(covers[1]);
                network.coverage.any = tier_area(covers[2]);
                network
            })
            .collect();
        for (network, covers) in networks.iter().zip(&tiers) {
            prop_assert_eq!(network.coverage.tier_at(position()), best(covers));
        }

        let rank = |tier: CoverageTier| match tier {
            CoverageTier::Realtime => 0,
            CoverageTier::Regular => 1,
            _ => 2,
        };
        let mut expected: Vec<(String, CoverageTier)> = tiers
            .iter()
            .enumerate()
            .filter_map(|(index, covers)| Some((index.to_string(), best(covers)?)))
            .collect();
        expected.sort_by_key(|(_, tier)| rank(*tier));

        let covering: Vec<_> = Registry::new(networks)
            .covering(position())
            .into_iter()
            .map(|covering| (covering.network.id, covering.tier))
            .collect();
        prop_assert_eq!(covering, expected);
    }
}

#[test]
fn registry_keeps_the_given_networks() {
    let networks: Vec<Network> = ["b", "a", "c"]
        .into_iter()
        .map(|id| Network::new(id, id.to_uppercase(), "https://example.org", Protocol::Efa))
        .collect();
    let registry = Registry::new(networks.clone());
    assert_eq!(registry.networks(), networks);
    assert_eq!(registry.get("a"), Some(networks[1].clone()));
    assert_eq!(registry.get("d"), None);
}

#[test]
fn bundled_networks_are_usable() {
    let registry = Registry::bundled();
    let networks = registry.networks();
    let mut found = ids(&networks);
    found.sort_unstable();
    assert_eq!(
        found,
        [
            "transitous",
            "vbb-hafas",
            "vrr-efa",
            "vrr-ojp",
            "vrr-trias",
            "vvo-efa",
            "vvo-ojp",
            "vvo-trias"
        ]
    );
    for network in &networks {
        let request = network.locations_request(&LocationQuery::new("Hauptbahnhof"));
        assert!(request.is_ok(), "{}: {request:?}", network.id);
    }

    let vvo_ojp = registry.get("vvo-ojp").expect("vvo-ojp");
    assert!(refuses(&vvo_ojp, Operation::Departures));

    let covering = |latitude, longitude| -> Vec<String> {
        let covering = registry.covering(at(latitude, longitude));
        covering.into_iter().map(|c| c.network.id).collect()
    };
    let dresden = covering(51.05, 13.74);
    for id in ["vvo-efa", "vvo-trias", "vvo-ojp", "transitous"] {
        assert!(dresden.iter().any(|d| d == id), "{id} in Dresden");
    }
    let duesseldorf = covering(51.22, 6.79);
    for id in ["vrr-efa", "vrr-trias", "vrr-ojp"] {
        assert!(duesseldorf.iter().any(|d| d == id), "{id} in Düsseldorf");
    }
}

#[test]
fn bundled_transitous_alone_names_usage_terms() {
    let terms: Vec<(String, String)> = Registry::bundled()
        .networks()
        .into_iter()
        .filter_map(|network| Some((network.id, network.terms?)))
        .collect();
    assert!(
        matches!(terms.as_slice(), [(id, terms)] if id == "transitous" && !terms.is_empty()),
        "{terms:?}"
    );
}
