//! Lists the next departures at a stop on the HAFAS server of VBB, the
//! transport association of Berlin and Brandenburg.

use publictransport_client::{
    BundledNetwork, Client, DeparturesQuery, Location, LocationKind, LocationQuery, Network,
    Timestamp,
};

// Servers want to know who asks: name your application and a way to reach you.
const USER_AGENT: &str = "publictransport-example (+https://github.com/kiliankoe/publictransport)";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = Network::bundled(BundledNetwork::VbbHafas);
    let client = Client::new(USER_AGENT)?;

    let query = LocationQuery::new("S+U Alexanderplatz").with_kinds([LocationKind::Stop]);
    let stops = client.locations(&network, &query).await?;
    let stop = stops.first().ok_or("found no stop")?;

    // From now on. `with_time` asks for another time.
    let query = DeparturesQuery::new(stop).with_limit(10);
    println!("Departures at {}:", stop.name);
    for departure in client.departures(&network, &query).await? {
        // Only departures with real-time data have a delay.
        let delay = departure.realtime_time.map_or_else(String::new, |time| {
            let seconds = time.unix_seconds - departure.scheduled_time.unix_seconds;
            format!("{:+}", seconds / 60)
        });
        println!(
            "  {} {delay:>3} {:<5} {}{}",
            local_time(&departure.stop, departure.scheduled_time),
            departure.line.name,
            departure.direction.as_deref().unwrap_or("?"),
            if departure.cancelled {
                " (cancelled)"
            } else {
                ""
            },
        );
    }
    Ok(())
}

/// The time of day on the clocks at the stop. Times are in UTC, and a
/// location knows the time zone to show them in, where the server names it.
fn local_time(stop: &Location, time: Timestamp) -> String {
    stop.local_time(time)
        .map_or_else(|| "??:??".to_owned(), |time| time.to_string())
}
