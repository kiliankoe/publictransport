//! The JSON interface (`outputFormat=rapidJSON`) of MDV's EFA journey planner.

use serde::Deserialize;

use crate::{
    Coordinate, Error, HttpRequest, HttpResponse, Location, LocationKind, LocationQuery, Network,
};

pub(crate) fn locations_request(network: &Network, query: &LocationQuery) -> HttpRequest {
    let limit = query.limit.map(|limit| limit.to_string());
    let mut parameters = vec![
        ("outputFormat", "rapidJSON"),
        // The default is a projected coordinate system.
        ("coordOutputFormat", "WGS84[dd.ddddd]"),
        ("type_sf", "any"),
        ("name_sf", query.text.as_str()),
    ];
    if let Some(limit) = &limit {
        parameters.push(("anyMaxSizeHitList", limit));
    }
    HttpRequest::get(&network.endpoint, "XML_STOPFINDER_REQUEST", parameters)
}

pub(crate) fn parse_locations(response: &HttpResponse) -> Result<Vec<Location>, Error> {
    // `systemMessages` is not checked: the server reports an ambiguous search
    // as an error there and still returns the matches.
    let mut response: StopFinderResponse = response.json()?;
    // Servers do not promise an order.
    response
        .locations
        .sort_by_key(|location| std::cmp::Reverse(location.match_quality));
    Ok(response.locations.into_iter().map(Location::from).collect())
}

#[derive(Deserialize)]
struct StopFinderResponse {
    locations: Vec<WireLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLocation {
    id: String,
    is_global_id: Option<bool>,
    /// Includes the locality, e.g. "Dresden, Hauptbahnhof".
    name: String,
    disassembled_name: Option<String>,
    /// Latitude, then longitude.
    coord: Option<[f64; 2]>,
    #[serde(rename = "type")]
    kind: String,
    parent: Option<Parent>,
    #[serde(default)]
    match_quality: i64,
}

#[derive(Deserialize)]
struct Parent {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

impl From<WireLocation> for Location {
    fn from(wire: WireLocation) -> Self {
        Self {
            global_id: (wire.is_global_id == Some(true)).then(|| wire.id.clone()),
            id: wire.id,
            kind: match wire.kind.as_str() {
                "stop" => LocationKind::Stop,
                "street" | "singlehouse" => LocationKind::Address,
                "poi" => LocationKind::Poi,
                _ => LocationKind::Other,
            },
            name: wire
                .disassembled_name
                .filter(|name| !name.is_empty())
                .unwrap_or(wire.name),
            locality: wire
                .parent
                .filter(|parent| parent.kind.as_deref() == Some("locality"))
                .and_then(|parent| parent.name),
            coordinate: wire.coord.map(|[latitude, longitude]| Coordinate {
                latitude,
                longitude,
            }),
        }
    }
}
