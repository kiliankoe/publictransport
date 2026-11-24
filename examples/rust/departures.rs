//! Lists the next departures at a stop on the HAFAS server of VBB, the
//! transport association of Berlin and Brandenburg.

use publictransport_client::{
    BundledNetwork, Client, DeparturesQuery, LocationKind, LocationQuery, Network, RemarkKind,
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
    let departures = client.departures(&network, &query).await?;
    for departure in &departures {
        // Only departures with real-time data have a delay.
        let delay = departure
            .delay_seconds()
            .map_or_else(String::new, |seconds| format!("{:+}", seconds / 60));
        // Times are in UTC. A departure also gives them as on the clocks at
        // its stop, where the server names the time zone.
        let time = departure
            .local_scheduled_time()
            .map_or_else(|| "??:??".to_owned(), |time| time.to_string());
        println!(
            "  {time} {delay:>3} {:<5} {}{}",
            departure.line.name,
            departure.direction.as_deref().unwrap_or("?"),
            if departure.cancelled {
                " (cancelled)"
            } else {
                ""
            },
        );
        // What the operator reports about the ride, such as a closed road.
        for remark in &departure.remarks {
            if let (RemarkKind::Disruption, Some(title)) = (remark.kind, &remark.title) {
                println!("            ! {title}");
            }
        }
    }

    // The run of the first vehicle: every stop it calls at. The ID only works
    // on the network it came from, and for about a day.
    let id = departures
        .iter()
        .find_map(|each| each.journey_id.as_deref());
    if let Some(id) = id {
        let journey = client.journey(&network, id).await?;
        println!("Stops of {}:", journey.line.name);
        for stopover in &journey.stops {
            // Each side of a stopover is an end of a leg, with the helpers
            // of a departure. The last stop has no departure.
            let time = stopover
                .departure()
                .or_else(|| stopover.arrival())
                .and_then(|end| end.local_expected_time())
                .map_or_else(|| "??:??".to_owned(), |time| time.to_string());
            println!("  {time} {}", stopover.stop.name);
        }
    }
    Ok(())
}
