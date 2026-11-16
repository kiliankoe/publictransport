//! The MOTIS API, as served by Transitous.

use serde::Deserialize;

use crate::{
    Coordinate, Error, HttpRequest, HttpResponse, Location, LocationKind, LocationQuery, Network,
};

pub(crate) fn locations_request(network: &Network, query: &LocationQuery) -> HttpRequest {
    let limit = query.limit.map(|limit| limit.to_string());
    let mut parameters = vec![("text", query.text.as_str())];
    if let Some(limit) = &limit {
        parameters.push(("numResults", limit));
    }
    HttpRequest::get(&network.endpoint, "v1/geocode", parameters)
}

pub(crate) fn parse_locations(response: &HttpResponse) -> Result<Vec<Location>, Error> {
    let matches: Vec<Match> = response.json()?;
    Ok(matches.into_iter().map(Location::from).collect())
}

#[derive(Deserialize)]
struct Match {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    id: String,
    lat: f64,
    lon: f64,
    #[serde(default)]
    areas: Vec<Area>,
}

#[derive(Deserialize)]
struct Area {
    name: String,
    /// Marks the area to show next to the name, usually the municipality.
    #[serde(default)]
    default: bool,
}

impl From<Match> for Location {
    fn from(wire: Match) -> Self {
        let kind = match wire.kind.as_str() {
            "STOP" => LocationKind::Stop,
            "ADDRESS" => LocationKind::Address,
            "PLACE" => LocationKind::Poi,
            _ => LocationKind::Other,
        };
        Self {
            // Other kinds carry OpenStreetMap identifiers.
            global_id: (kind == LocationKind::Stop)
                .then(|| global_id(&wire.id).map(str::to_owned))
                .flatten(),
            id: wire.id,
            kind,
            name: wire.name,
            locality: wire
                .areas
                .into_iter()
                .find(|area| area.default)
                .map(|area| area.name),
            coordinate: Some(Coordinate {
                latitude: wire.lat,
                longitude: wire.lon,
            }),
        }
    }
}

/// The IFOPT identifier inside a stop ID of the form `<feed>_<id in feed>`.
/// Feed names and IFOPT identifiers may both contain underscores, so every
/// split is tried.
fn global_id(id: &str) -> Option<&str> {
    id.match_indices('_')
        .filter_map(|(index, _)| id.get(index + 1..))
        .find(|candidate| is_ifopt(candidate))
}

/// Whether `id` looks like `de:14612:28`, `de:14612:28:3:2` or
/// `ch:1:sloid:7000`: a country code, a number, the stop, then up to two more
/// parts for area and platform.
fn is_ifopt(id: &str) -> bool {
    let parts: Vec<&str> = id.split(':').collect();
    let [country, region, stop, rest @ ..] = parts.as_slice() else {
        return false;
    };
    country.len() == 2
        && country.bytes().all(|b| b.is_ascii_lowercase())
        && !region.is_empty()
        && region.bytes().all(|b| b.is_ascii_digit())
        && !stop.is_empty()
        && rest.len() <= 2
        && rest.iter().all(|part| !part.is_empty())
}
