//! Searches two stops, lists the next departures at the first one and plans
//! a trip between them, against a live server.
//!
//! ```sh
//! cargo run --example trip -- vvo-efa "Dresden Hauptbahnhof" "Dresden Albertplatz"
//! cargo run --example trip -- transitous "Dresden Hauptbahnhof" "Dresden Albertplatz"
//! cargo run --example trip -- vvo-trias "Dresden Hauptbahnhof" "Dresden Albertplatz"
//! cargo run --example trip -- vrr-ojp "Düsseldorf Hbf" "Essen Hbf"
//! cargo run --example trip -- vbb-hafas "S+U Alexanderplatz" "S+U Zoologischer Garten"
//! cargo run --example trip -- entur "Oslo S" "Bergen stasjon"
//! ```

use std::error::Error;

use publictransport_client::{
    Client, DeparturesQuery, LegEnd, LegKind, Location, LocationKind, LocationQuery, Network,
    Operation, Place, Registry, TripsQuery,
};

const USER_AGENT: &str = concat!(
    "publictransport-example/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/kiliankoe/publictransport)"
);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [id, from, to] = arguments.as_slice() else {
        return Err("usage: trip <network id> <from> <to>".into());
    };
    let registry = Registry::bundled();
    let Some(network) = registry.get(id) else {
        let ids: Vec<_> = registry
            .networks()
            .into_iter()
            .map(|network| network.id)
            .collect();
        return Err(format!("unknown network {id}, known are {}", ids.join(", ")).into());
    };
    // Entur asks every client for its name.
    let network = if network.id == "entur" {
        network.with_header("ET-Client-Name", "publictransport-example")
    } else {
        network
    };
    let client = Client::new(USER_AGENT)?;

    let from = first_stop(&client, &network, from).await?;
    let to = first_stop(&client, &network, to).await?;

    // Some servers have no departures.
    if !network.unsupported.contains(&Operation::Departures) {
        println!("Departures at {}:", from.name);
        let query = DeparturesQuery::new(&from.id).with_limit(5);
        for departure in client.departures(&network, &query).await? {
            let delay = departure
                .realtime_time
                .map(|time| time.unix_seconds - departure.scheduled_time.unix_seconds);
            println!(
                "  {} {:>5} {:<6} {}{}",
                local_time(&departure.stop, departure.scheduled_time.unix_seconds),
                delay.map_or_else(String::new, |seconds| format!("{:+}", seconds / 60)),
                departure.line.name,
                departure.direction.as_deref().unwrap_or("?"),
                if departure.cancelled {
                    " (cancelled)"
                } else {
                    ""
                },
            );
        }
    }

    println!("Trips from {} to {}:", from.name, to.name);
    let query = TripsQuery::new(Place::Id(from.id), Place::Id(to.id)).with_limit(3);
    for trip in client.trips(&network, &query).await?.trips {
        let (Some(departure), Some(arrival)) = (trip.departure(), trip.arrival()) else {
            continue;
        };
        println!(
            "  {} to {}, {} change(s)",
            end_time(departure),
            end_time(arrival),
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
                end_time(&leg.departure),
                leg.departure.stop.name,
                end_time(&leg.arrival),
                leg.arrival.stop.name,
            );
        }
    }
    Ok(())
}

async fn first_stop(
    client: &Client,
    network: &Network,
    text: &str,
) -> Result<Location, Box<dyn Error>> {
    // Without a limit: some servers find nothing when one cuts the list.
    let query = LocationQuery::new(text).with_kinds([LocationKind::Stop]);
    client
        .locations(network, &query)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| format!("no stop found for {text}").into())
}

fn end_time(end: &LegEnd) -> String {
    local_time(&end.stop, end.expected_time().unix_seconds)
}

/// The time of day at the stop, or in UTC if its time zone is unknown.
fn local_time(stop: &Location, unix_seconds: i64) -> String {
    let zone = stop
        .timezone
        .as_deref()
        .and_then(|name| jiff::tz::TimeZone::get(name).ok())
        .unwrap_or(jiff::tz::TimeZone::UTC);
    jiff::Timestamp::from_second(unix_seconds).map_or_else(
        |_| "??:??".to_owned(),
        |time| time.to_zoned_datetime(zone).strftime("%H:%M").to_string(),
    )
}
