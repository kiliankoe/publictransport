//! Asks live servers for stops, departures and trips and prints one line
//! per network, to see whether a network still works. It sends four
//! requests to a server that answers well. Where the stops of a search
//! have no departures or no trips between them it searches again, with up
//! to five words.
//!
//! Without arguments it leaves out Transitous, a service run by
//! volunteers, so that a scheduled run never asks it. Name it to check it.
//!
//! ```sh
//! # every bundled network but Transitous
//! cargo run --example check
//! # some of them, or entries in the format of transport-apis
//! cargo run --example check -- vvo-efa path/to/entry.json
//! ```

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;

use publictransport_client::{
    BundledNetwork, Client, DeparturesQuery, LegKind, LocationKind, LocationQuery, Mode, Network,
    Operation, Trips, TripsQuery,
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
            BundledNetwork::ALL
                .iter()
                .find(|network| network.id() == argument)
                .map(|network| Network::bundled(*network))
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
        networks = BundledNetwork::ALL
            .iter()
            .filter(|network| **network != BundledNetwork::Transitous)
            .map(|network| Network::bundled(*network))
            .collect();
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
/// A search can find stops without departures or without a way between
/// them, so each word is tried until the stops of one answer everything.
async fn check(client: &Client, network: &Network) -> Result<String, Box<dyn Error>> {
    // The first word finds the main stations, so its reason says the most.
    let mut unanswered = None;
    for text in SEARCHES {
        match check_with(client, network, text).await? {
            Ok(summary) => return Ok(summary),
            Err(reason) => unanswered = unanswered.or(Some(reason)),
        }
    }
    Err(unanswered.unwrap_or_default().into())
}

/// The check with the stops that a search for `text` finds. The inner
/// error says what these stops have no answer for; the outer one is a
/// failure of the server, which another search would not get around.
async fn check_with(
    client: &Client,
    network: &Network,
    text: &str,
) -> Result<Result<String, String>, Box<dyn Error>> {
    let supports = |operation| !network.unsupported.contains(&operation);
    // Without a limit: some servers find nothing when one cuts the list.
    let query = LocationQuery::new(text).with_kinds([LocationKind::Stop]);
    let stops = client
        .locations(network, &query)
        .await
        .map_err(|error| format!("locations: {error}"))?;
    let [from, near, .., far] = stops.as_slice() else {
        return Ok(Err(format!("{text} finds fewer than three stops")));
    };
    let mut summary = vec![format!("from {}", from.name)];
    let mut modes = BTreeSet::new();

    if supports(Operation::Departures) {
        let query = DeparturesQuery::new(from).with_limit(10);
        let departures = client
            .departures(network, &query)
            .await
            .map_err(|error| format!("departures: {error}"))?;
        if departures.is_empty() {
            return Ok(Err(format!("no departures at {}", from.name)));
        }
        let live = departures
            .iter()
            .filter(|departure| departure.realtime_time.is_some())
            .count();
        modes.extend(departures.iter().map(|departure| name(departure.line.mode)));
        summary.push(format!("{} departures ({live} live)", departures.len()));
    }
    if supports(Operation::Trips) {
        // A search finds stops in no order. The server may know no way
        // to one of them, or count it as the same station as the first.
        let mut outcome = Err("no destination".to_owned());
        let mut failure = None;
        for to in [near, far] {
            let query = TripsQuery::new(from, to).with_limit(2);
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
                Err(error) => {
                    failure = Some(format!("trips to {}: {error}", to.name));
                    Err(String::new())
                }
            };
            if outcome.is_ok() {
                break;
            }
        }
        let (query, found, to) = match outcome {
            Ok(found) => found,
            // The server failed, and not only for one pair of stops.
            Err(_) if failure.is_some() => return Err(failure.unwrap_or_default().into()),
            Err(reason) => return Ok(Err(reason)),
        };
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
    Ok(Ok(summary.join(", ")))
}

fn name(mode: Mode) -> String {
    format!("{mode:?}")
}
