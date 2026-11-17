//! Writes one request of every kind for TRIAS and OJP into a directory, for
//! `scripts/validate-xml-requests.sh` to check against the official schemas.

use std::{error::Error, fs, path::PathBuf};

use publictransport::{
    Coordinate, DeparturesQuery, HttpRequest, LocationKind, LocationQuery, Network, Place,
    Protocol, Timestamp, TripsQuery,
};

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

    for (name, protocol) in [("trias", Protocol::Trias), ("ojp", Protocol::Ojp1)] {
        let network = Network::new(name, name, "https://example.org", protocol)
            .with_requestor_ref("OpenService");
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
        ];
        for (kind, HttpRequest { body, .. }) in requests {
            let file = directory.join(format!("{name}-{kind}.xml"));
            fs::write(file, body.ok_or("the request has no body")?)?;
        }
    }
    Ok(())
}
