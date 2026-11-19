//! Prints the page of the documentation site that lists the bundled
//! networks, as Markdown. `just site` writes it into the site.

use publictransport::{Operation, Protocol, Registry};

fn main() {
    println!("# Bundled networks\n");
    println!(
        "These networks ship with the library. Each was checked against its live server. \
         The ID finds a network in the registry, and `BundledNetwork` has a name for each, spelled like the ID.\n"
    );
    println!("| Network | ID | Protocol | Notes |");
    println!("| --- | --- | --- | --- |");
    for network in Registry::bundled().networks() {
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
        println!(
            "| {} | `{}` | {protocol} | {} |",
            network.name,
            network.id,
            notes.join(", ")
        );
    }
}
