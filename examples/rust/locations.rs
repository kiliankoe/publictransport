//! Searches for places by name on the EFA server of VVO, the transport
//! association of Dresden.

use publictransport_client::{BundledNetwork, Client, LocationQuery, Network};

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
    Ok(())
}
