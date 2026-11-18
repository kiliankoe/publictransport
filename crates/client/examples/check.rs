//! Asks live servers for stops, departures and trips and prints one line
//! per network, to see whether a network still works. It sends four
//! requests to a server that answers well and a few more to one whose
//! first stops have no departures or no trips between them.
//!
//! ```sh
//! # every bundled network
//! cargo run --example check
//! # some of them, or entries in the format of transport-apis
//! cargo run --example check -- vvo-efa path/to/entry.json
//! ```

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;

use publictransport_client::{
    Client, DeparturesQuery, LegKind, LocationKind, LocationQuery, Mode, Network, Operation,
    Registry, Trips, TripsQuery,
};

const USER_AGENT: &str = concat!(
    "publictransport-example/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/kiliankoe/publictransport)"
);

/// Words that find stops with departures on most servers, tried in this
/// order.
const SEARCHES: [&str; 5] = [
    "Hauptbahnhof",
    "Gare",
    "Centralstation",
    "Central",
    "Station",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let registry = Registry::bundled();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut networks = Vec::new();
    let mut failed = 0;
    for argument in &arguments {
        let path = Path::new(argument);
        let network = if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let id = path.file_stem().and_then(|stem| stem.to_str());
            let json = std::fs::read_to_string(path)?;
            Network::from_json(id.unwrap_or("entry"), &json).map_err(|error| error.to_string())
        } else {
            registry
                .get(argument)
                .ok_or_else(|| "not a bundled network".to_owned())
        };
        match network {
            Ok(network) => networks.push(network),
            Err(error) => {
                failed += 1;
                println!("{argument}: FAILED, {error}");
            }
        }
    }
    if arguments.is_empty() {
        networks = registry.networks();
    }

    let client = Client::new(USER_AGENT)?;
    for network in networks {
        // Entur asks every client for its name.
        let network = &if network.id == "entur" {
            network.with_header("ET-Client-Name", "publictransport-example")
        } else {
            network
        };
        match check(&client, network).await {
            Ok(summary) => println!("{}: ok, {summary}", network.id),
            Err(error) => {
                failed += 1;
                println!("{}: FAILED, {error}", network.id);
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} networks failed").into());
    }
    Ok(())
}

/// Runs every operation the network supports and describes the answers.
async fn check(client: &Client, network: &Network) -> Result<String, Box<dyn Error>> {
    let supports = |operation| !network.unsupported.contains(&operation);
    let mut found = None;
    for text in SEARCHES {
        // Without a limit: some servers find nothing when one cuts the list.
        let query = LocationQuery::new(text).with_kinds([LocationKind::Stop]);
        let stops = client
            .locations(network, &query)
            .await
            .map_err(|error| format!("locations: {error}"))?;
        let [from, near, .., far] = stops.as_slice() else {
            continue;
        };
        let departures = if supports(Operation::Departures) {
            let query = DeparturesQuery::new(from).with_limit(10);
            client
                .departures(network, &query)
                .await
                .map_err(|error| format!("departures: {error}"))?
        } else {
            Vec::new()
        };
        // A search can find stops the server has no timetable for.
        let done = !departures.is_empty() || !supports(Operation::Departures);
        found = Some((from.clone(), [near.clone(), far.clone()], departures));
        if done {
            break;
        }
    }
    let Some((from, destinations, departures)) = found else {
        return Err("no search found three stops".into());
    };
    let mut summary = vec![format!("from {}", from.name)];
    let mut modes = BTreeSet::new();

    if supports(Operation::Departures) {
        let live = departures
            .iter()
            .filter(|departure| departure.realtime_time.is_some())
            .count();
        modes.extend(departures.iter().map(|departure| name(departure.line.mode)));
        if departures.is_empty() {
            return Err("no stop of any search has departures".into());
        }
        summary.push(format!("{} departures ({live} live)", departures.len()));
    }
    if supports(Operation::Trips) {
        // A search finds stops in no order. The server may know no way
        // to one of them, or count it as the same station as the first.
        let mut outcome = Err("no destination".to_owned());
        for to in &destinations {
            let query = TripsQuery::new(&from, to).with_limit(2);
            let mut answer = client.trips(network, &query).await;
            // OpenTripPlanner searches a window of time and answers with
            // no trips and a reference if the window has none, as at night.
            if let Ok(Trips {
                trips,
                later: Some(later),
                ..
            }) = &answer
                && trips.is_empty()
            {
                answer = client.trips(network, &query.clone().with_page(later)).await;
            }
            outcome = match answer {
                Ok(found) if found.trips.is_empty() => Err(format!("no trips to {}", to.name)),
                Ok(found) => Ok((query, found, &to.name)),
                Err(error) => Err(format!("trips to {}: {error}", to.name)),
            };
            if outcome.is_ok() {
                break;
            }
        }
        let (query, found, to) = outcome?;
        let legs = found.trips.iter().flat_map(|trip| &trip.legs);
        modes.extend(legs.filter_map(|leg| match &leg.kind {
            LegKind::Transit { line, .. } => Some(name(line.mode)),
            _ => None,
        }));
        let mut more = "no more";
        if let Some(later) = found.later {
            client
                .trips(network, &query.with_page(later))
                .await
                .map_err(|error| format!("later trips: {error}"))?;
            more = "more";
        }
        summary.push(format!("{} trips to {to} ({more})", found.trips.len()));
    }
    let modes: Vec<_> = modes.into_iter().collect();
    summary.push(format!("modes {}", modes.join(" ")));
    Ok(summary.join(", "))
}

fn name(mode: Mode) -> String {
    format!("{mode:?}")
}
