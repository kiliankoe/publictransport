//! Prints the page of the documentation site that lists the bundled
//! networks, as Markdown. `just site` writes it into the site.

use publictransport::{BundledNetwork, Network, Operation, Protocol, Registry};

fn main() {
    println!("# Bundled networks\n");
    println!(
        "These networks ship with the library. Each was checked against its live server. \
         `BundledNetwork` has a name for each, spelled like the ID. The registry has one network for each operator, \
         found by its ID: where an operator runs servers for several protocols, it has the one whose answers hold the most.\n"
    );
    println!("| Network | ID | Protocol | Notes |");
    println!("| --- | --- | --- | --- |");
    let registry = Registry::bundled();
    for network in BundledNetwork::ALL
        .iter()
        .map(|name| Network::bundled(*name))
    {
        let protocol = match network.protocol {
            Protocol::Efa => "EFA",
            Protocol::Motis => "MOTIS",
            Protocol::Trias => "TRIAS 1.2",
            Protocol::Ojp1 => "OJP 1.0",
            Protocol::HafasMgate => "HAFAS mgate",
            Protocol::OtpTransmodel => "OpenTripPlanner",
            _ => "other",
        };
        let mut notes = Vec::new();
        if network.unsupported.contains(&Operation::Departures) {
            notes.push("no departures");
        }
        if network.unsupported.contains(&Operation::Trips) {
            notes.push("no trips");
        }
        if network.requires_key {
            notes.push("needs a key");
        }
        if registry.get(&network.id).is_none() {
            notes.push("not in the registry");
        }
        println!(
            "| {} | `{}` | {protocol} | {} |",
            network.name,
            network.id,
            notes.join(", ")
        );
    }
    println!("{NOTES}");
}

/// What holds for every language, so that the guides do not each repeat it.
const NOTES: &str = "
## A network of your own

A network that is not bundled can be described in code or read from an entry in the format of [transport-apis](https://github.com/public-transport/transport-apis). What it needs besides its endpoint depends on the protocol. The names are those of the Rust fields, which each language spells in its own case.

| Protocol | Needs |
| --- | --- |
| EFA | `timezone`, to send a query time |
| TRIAS, OJP | `requestor_ref`, which public servers document, e.g. `OpenService` |
| HAFAS mgate | `hafas` with the options of the server, and `timezone` |
| OpenTripPlanner | `geocoder`, the server that searches for locations |

A server that wants a key in an HTTP header gets it through `headers`. A `geocoder` gets these headers only if it has the scheme, host and port of the endpoint, so that a key does not reach another party.

## Good to know

- Entur asks every client for its name in the header `ET-Client-Name`, as `company-application`, and limits clients without it. Add it to the `headers` of the bundled network.
- An answer to a trips query from an OpenTripPlanner server can be empty at night and still have a `later` reference to follow.
- A location that another network returned works in a query too: for a trip always if it has a coordinate, for departures where the asked network can name the stop, which EFA, TRIAS and OJP can for stops with a global ID.
- The limit of a query is a guide for the server and not a maximum.
- The clients do not retry, limit their request rate or cache. They do not follow redirects either: a server that has moved is an error.";
