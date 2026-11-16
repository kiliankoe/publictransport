//! The MOTIS API, as served by Transitous.

use serde::Deserialize;

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, HttpRequest, HttpResponse, Line, Location,
    LocationKind, LocationQuery, Mode, Network, time::WireTime,
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

/// Whether `id` looks like `de:14612:28` or `de:14612:28:3:2`: a country
/// code, two numbers, then up to two more parts for area and platform.
fn is_ifopt(id: &str) -> bool {
    let parts: Vec<&str> = id.split(':').collect();
    let [country, region, stop, rest @ ..] = parts.as_slice() else {
        return false;
    };
    let is_number = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    country.len() == 2
        && country.bytes().all(|b| b.is_ascii_lowercase())
        && is_number(region)
        && is_number(stop)
        && rest.len() <= 2
        && rest.iter().all(|part| !part.is_empty())
}

pub(crate) fn departures_request(
    network: &Network,
    query: &DeparturesQuery,
) -> Result<HttpRequest, Error> {
    // The server needs `n` or a time window.
    let limit = query.limit.unwrap_or(10).to_string();
    let time = query
        .time
        .map(|time| time.to_jiff().map(|time| time.to_string()))
        .transpose()?;
    let mut parameters = vec![("stopId", query.stop_id.as_str()), ("n", limit.as_str())];
    if let Some(time) = &time {
        parameters.push(("time", time));
    }
    Ok(HttpRequest::get(
        &network.endpoint,
        "v6/stoptimes",
        parameters,
    ))
}

pub(crate) fn parse_departures(response: &HttpResponse) -> Result<Vec<Departure>, Error> {
    // An unknown stop is a 404 with a JSON body, unlike a wrong endpoint.
    if response.status == 404
        && let Ok(ErrorBody { error }) = serde_json::from_slice(&response.body)
    {
        return Err(Error::NotFound { message: error });
    }
    let response: StopTimesResponse = response.json()?;
    let mut departures: Vec<_> = response
        .stop_times
        .into_iter()
        .map(Departure::from)
        .collect();
    departures.sort_by_key(Departure::expected_time);
    Ok(departures)
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopTimesResponse {
    stop_times: Vec<StopTime>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopTime {
    place: Place,
    mode: String,
    /// Whether `place.departure` is a prediction. Without one the server
    /// repeats the scheduled time there.
    #[serde(default)]
    real_time: bool,
    headsign: Option<String>,
    agency_name: Option<String>,
    display_name: Option<String>,
    route_short_name: Option<String>,
    #[serde(default)]
    cancelled: bool,
    #[serde(default)]
    trip_cancelled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Place {
    stop_id: String,
    name: String,
    lat: f64,
    lon: f64,
    departure: Option<WireTime>,
    scheduled_departure: WireTime,
    track: Option<String>,
    scheduled_track: Option<String>,
    #[serde(default)]
    cancelled: bool,
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
}

impl From<StopTime> for Departure {
    fn from(wire: StopTime) -> Self {
        Self {
            line: Line {
                name: non_empty(wire.display_name)
                    .or(non_empty(wire.route_short_name))
                    .unwrap_or_default(),
                mode: mode(&wire.mode),
                product: None,
                operator: non_empty(wire.agency_name),
            },
            direction: non_empty(wire.headsign),
            scheduled_time: wire.place.scheduled_departure.0,
            realtime_time: wire
                .place
                .departure
                .filter(|_| wire.real_time)
                .map(|time| time.0),
            scheduled_platform: non_empty(wire.place.scheduled_track),
            realtime_platform: non_empty(wire.place.track),
            cancelled: wire.cancelled || wire.trip_cancelled || wire.place.cancelled,
            stop: Location {
                global_id: global_id(&wire.place.stop_id).map(str::to_owned),
                id: wire.place.stop_id,
                kind: LocationKind::Stop,
                name: wire.place.name,
                locality: None,
                coordinate: Some(Coordinate {
                    latitude: wire.place.lat,
                    longitude: wire.place.lon,
                }),
            },
        }
    }
}

fn mode(mode: &str) -> Mode {
    match mode {
        "HIGHSPEED_RAIL" => Mode::HighSpeedTrain,
        "LONG_DISTANCE" => Mode::LongDistanceTrain,
        "NIGHT_RAIL" => Mode::NightTrain,
        "REGIONAL_RAIL" | "REGIONAL_FAST_RAIL" => Mode::RegionalTrain,
        "SUBURBAN" => Mode::SuburbanTrain,
        "SUBWAY" => Mode::Subway,
        "TRAM" => Mode::Tram,
        "BUS" => Mode::Bus,
        "COACH" => Mode::Coach,
        "FERRY" => Mode::Ferry,
        "AIRPLANE" => Mode::Airplane,
        "FUNICULAR" => Mode::Funicular,
        "AERIAL_LIFT" => Mode::CableCar,
        "ODM" => Mode::OnDemand,
        _ => Mode::Other,
    }
}
