//! Lists the bundled networks that cover a position. This sends no
//! request: the networks and their areas ship with the library.

use publictransport_client::{Coordinate, Registry};

fn main() {
    let registry = Registry::bundled();
    println!("{} networks are bundled.", registry.networks().len());

    let duesseldorf = Coordinate {
        latitude: 51.2198,
        longitude: 6.7943,
    };
    // The best coverage comes first: real-time data, then complete
    // timetables, then any data.
    println!("At Düsseldorf Hauptbahnhof:");
    for covering in registry.covering(duesseldorf) {
        let network = covering.network;
        println!(
            "  {:?}: {} ({}, {:?})",
            covering.tier, network.name, network.id, network.protocol
        );
    }

    // A network by its ID, as an application would store it.
    if let Some(network) = registry.get("vrr-efa") {
        println!("vrr-efa is {} at {}", network.name, network.endpoint);
    }
}
