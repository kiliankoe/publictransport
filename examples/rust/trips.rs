//! Plans trips between two stops on the OTP server of Entur, the journey
//! planner for Norway, and asks for the trips after the first ones.

use publictransport_client::{
    BundledNetwork, Client, LegEnd, LegKind, Location, LocationKind, LocationQuery, Network, Trip,
    TripsQuery,
};

// Servers want to know who asks: name your application and a way to reach you.
const USER_AGENT: &str = "publictransport-example (+https://github.com/kiliankoe/publictransport)";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Entur asks every client for its name, as `company-application`.
    let network = Network::bundled(BundledNetwork::Entur)
        .with_header("ET-Client-Name", "publictransport-example");
    let client = Client::new(USER_AGENT)?;

    let from = first_stop(&client, &network, "Oslo S").await?;
    let to = first_stop(&client, &network, "Lillehammer stasjon").await?;

    // Leaving now. `with_departure` and `with_arrival` ask for another time.
    // An end can also be a coordinate or the ID of a stop.
    let query = TripsQuery::new(&from, &to).with_limit(3);
    let found = client.trips(&network, &query).await?;
    println!("Trips from {} to {}:", from.name, to.name);
    found.trips.iter().for_each(print_trip);

    // The same query again, with the reference to the trips after these.
    if let Some(later) = found.later {
        println!("Later trips:");
        let more = client.trips(&network, &query.with_page(later)).await?;
        more.trips.iter().for_each(print_trip);
    }
    Ok(())
}

async fn first_stop(
    client: &Client,
    network: &Network,
    text: &str,
) -> Result<Location, Box<dyn std::error::Error>> {
    let query = LocationQuery::new(text).with_kinds([LocationKind::Stop]);
    let stops = client.locations(network, &query).await?;
    stops
        .into_iter()
        .next()
        .ok_or_else(|| format!("found no stop for {text}").into())
}

fn print_trip(trip: &Trip) {
    let (Some(departure), Some(arrival), Some(duration)) =
        (trip.departure(), trip.arrival(), trip.duration_seconds())
    else {
        return;
    };
    println!(
        "  {} to {}, {} min, {} change(s)",
        local_time(departure),
        local_time(arrival),
        duration / 60,
        trip.transfers()
    );
    for leg in &trip.legs {
        let how = match &leg.kind {
            LegKind::Transit { line, direction } => {
                format!("{} to {}", line.name, direction.as_deref().unwrap_or("?"))
            }
            LegKind::Walk => "walk".to_owned(),
            _ => "other".to_owned(),
        };
        println!(
            "    {} {} -> {} {}: {how}",
            local_time(&leg.departure),
            leg.departure.stop.name,
            local_time(&leg.arrival),
            leg.arrival.stop.name,
        );
    }
}

/// The expected time of day on the clocks at the stop: the real-time one
/// if there is one. Times are in UTC, and an end of a leg also gives them
/// as on the clocks at its stop, where the server names the time zone.
fn local_time(end: &LegEnd) -> String {
    end.local_expected_time()
        .map_or_else(|| "??:??".to_owned(), |time| time.to_string())
}
