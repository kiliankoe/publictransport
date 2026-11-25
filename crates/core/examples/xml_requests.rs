//! Writes one request of every kind for TRIAS and OJP into a directory, for
//! `scripts/validate-xml-requests.sh` to check against the official schemas.

use std::{error::Error, fs, path::PathBuf};

use publictransport::{
    Coordinate, DeparturesQuery, HttpRequest, HttpResponse, LocationKind, LocationQuery,
    NearbyQuery, Network, Place, Protocol, Timestamp, TripsQuery,
};

/// An answer with a departure per protocol: only an answer names a run.
const TRIAS_DEPARTURE: &[u8] = include_bytes!("../tests/fixtures/trias/vvo-journey-departure.xml");
const OJP_DEPARTURE: &[u8] = include_bytes!("../tests/fixtures/ojp/vrr-journey-departure.xml");

fn main() -> Result<(), Box<dyn Error>> {
    let directory = PathBuf::from(std::env::args().nth(1).ok_or("usage: xml_requests <dir>")?);
    let time = Timestamp {
        unix_seconds: 1_794_835_440,
    };
    let stop = || Place::Id("de:14612:28".to_owned());
    let position = Place::Coordinate(Coordinate {
        latitude: 51.0633,
        longitude: 13.7467,
    });

    for (name, protocol, departure) in [
        ("trias", Protocol::Trias, TRIAS_DEPARTURE),
        ("ojp", Protocol::Ojp1, OJP_DEPARTURE),
    ] {
        let network = Network::new(name, name, "https://example.org", protocol)
            .with_requestor_ref("OpenService")
            .with_language("en");
        let departures = network.parse_departures(
            &publictransport::DeparturesQuery::new("stop"),
            &HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: departure.to_vec(),
            },
        )?;
        let run = departures
            .into_iter()
            .find_map(|departure| departure.journey_id)
            .ok_or("the recorded departure names no run")?;
        let requests = [
            (
                "locations",
                network.locations_request(
                    &LocationQuery::new("Haupt & Bahnhof")
                        .with_kinds([
                            LocationKind::Stop,
                            LocationKind::Address,
                            LocationKind::Poi,
                            LocationKind::Other,
                        ])
                        .with_limit(5),
                )?,
            ),
            (
                "departures",
                network.departures_request(
                    &DeparturesQuery::new("de:14612:28")
                        .with_time(time)
                        .with_limit(10),
                )?,
            ),
            (
                "arrivals",
                network.arrivals_request(&DeparturesQuery::new("de:14612:28").with_limit(10))?,
            ),
            (
                "nearby",
                network.nearby_request(
                    &NearbyQuery::new(Coordinate {
                        latitude: 51.04,
                        longitude: 13.7325,
                    })
                    .with_radius_meters(500)
                    .with_limit(5),
                )?,
            ),
            (
                "trips-from-stop",
                network.trips_request(
                    &TripsQuery::new(stop(), position.clone())
                        .with_departure(time)
                        .with_limit(3),
                )?,
            ),
            (
                "trips-to-stop",
                network.trips_request(
                    &TripsQuery::new(position.clone(), stop())
                        .with_arrival(time)
                        .with_limit(3),
                )?,
            ),
            ("journey", network.journey_request(&run)?),
        ];
        for (kind, HttpRequest { body, .. }) in requests {
            let file = directory.join(format!("{name}-{kind}.xml"));
            fs::write(file, body.ok_or("the request has no body")?)?;
        }
    }
    Ok(())
}
