//! Searches for places by name and near a position on the EFA server of
//! VVO, the transport association of Dresden.

use publictransport_client::{BundledNetwork, Client, LocationQuery, NearbyQuery, Network};

// Servers want to know who asks: name your application and a way to reach you.
const USER_AGENT: &str = "publictransport-example (+https://github.com/kiliankoe/publictransport)";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = Network::bundled(BundledNetwork::VvoEfa);
    let client = Client::new(USER_AGENT)?;

    // Every kind of place. `with_kinds([LocationKind::Stop])` asks for stops only.
    let query = LocationQuery::new("Dresden Zwinger");
    let locations = client.locations(&network, &query).await?;

    // The best match comes first.
    for location in locations.iter().take(5) {
        let position = location.coordinate.map_or_else(String::new, |coordinate| {
            format!(
                " at {:.4}, {:.4}",
                coordinate.latitude, coordinate.longitude
            )
        });
        println!(
            "{:?}: {} ({}){position}",
            location.kind, location.name, location.id
        );
    }

    // The stops within 300 m of the best match, the nearest first. The
    // distance is the straight line.
    let Some(center) = locations.first().and_then(|location| location.coordinate) else {
        return Ok(());
    };
    let query = NearbyQuery::new(center).with_radius_meters(300);
    for stop in client.nearby(&network, &query).await? {
        if let Some(coordinate) = stop.coordinate {
            println!("{:.0} m: {}", center.distance_meters(coordinate), stop.name);
        }
    }
    Ok(())
}
