//! The MOTIS API, as served by Transitous.

use serde::Deserialize;

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, GlobalId, HttpRequest, HttpResponse, Leg,
    LegKind, Line, Location, LocationKind, LocationQuery, Mode, Network, Stopover, Trip, TripTime,
    Trips, TripsQuery, model::Target, time::WireTime,
};

pub(crate) fn locations_request(
    network: &Network,
    query: &LocationQuery,
) -> Result<HttpRequest, Error> {
    let limit = query.limit.map(|limit| limit.to_string());
    // The server has no type for the other kinds.
    let types: Vec<&str> = [
        (LocationKind::Stop, "STOP"),
        (LocationKind::Address, "ADDRESS"),
        (LocationKind::Poi, "PLACE"),
    ]
    .into_iter()
    .filter(|(kind, _)| query.kinds.contains(kind))
    .map(|(_, name)| name)
    .collect();
    if types.is_empty() && !query.kinds.is_empty() {
        return Err(Error::InvalidQuery {
            message: "MOTIS has no locations of the kinds asked for".to_owned(),
        });
    }
    let types = types.join(",");
    let mut parameters = vec![("text", query.text.as_str())];
    if !types.is_empty() {
        parameters.push(("type", &types));
    }
    if let Some(limit) = &limit {
        parameters.push(("numResults", limit));
    }
    Ok(HttpRequest::get(
        &network.endpoint,
        "v1/geocode",
        parameters,
    ))
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
    tz: Option<String>,
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

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
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
            global_ids: (kind == LocationKind::Stop)
                .then(|| global_id(&wire.id))
                .flatten()
                .into_iter()
                .collect(),
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
            timezone: wire.tz,
            network: None,
        }
    }
}

/// The IFOPT identifier inside a stop ID of the form `<feed>_<id in feed>`.
/// Feed names and IFOPT identifiers may both contain underscores, so every
/// split is tried.
fn global_id(id: &str) -> Option<GlobalId> {
    id.match_indices('_')
        .filter_map(|(index, _)| id.get(index + 1..))
        .find_map(GlobalId::ifopt)
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
    let stop = query.stop.stop_id(network)?;
    let mut parameters = vec![("stopId", stop.as_str()), ("n", limit.as_str())];
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
    let response: StopTimesResponse = json(response)?;
    let mut departures = response
        .stop_times
        .into_iter()
        .map(StopTime::into_departure)
        .collect::<Result<Vec<_>, _>>()?;
    departures.sort_by_key(Departure::expected_time);
    Ok(departures)
}

pub(crate) fn trips_request(network: &Network, query: &TripsQuery) -> Result<HttpRequest, Error> {
    let from = place(query.origin.target(network)?);
    let to = place(query.destination.target(network)?);
    let limit = query.limit.map(|limit| limit.to_string());
    let (time, arrive_by) = match query.time {
        Some(TripTime::DepartAt(time)) => (Some(time), false),
        Some(TripTime::ArriveBy(time)) => (Some(time), true),
        None => (None, false),
    };
    let time = time
        .map(|time| time.to_jiff().map(|time| time.to_string()))
        .transpose()?;

    let mut parameters = vec![("fromPlace", from.as_str()), ("toPlace", to.as_str())];
    if let Some(limit) = &limit {
        // The first is the least number of trips. A page would hold all the
        // trips of two hours without the second.
        parameters.extend([
            ("numItineraries", limit.as_str()),
            ("maxItineraries", limit),
        ]);
    }
    if let Some(time) = &time {
        parameters.push(("time", time));
    }
    if arrive_by {
        parameters.push(("arriveBy", "true"));
    }
    // The server asks for the cursor on top of the request it answered.
    if let Some(page) = &query.page {
        parameters.push(("pageCursor", page));
    }
    Ok(HttpRequest::get(&network.endpoint, "v6/plan", parameters))
}

fn place(place: Target) -> String {
    match place {
        Target::Id(id) => id,
        Target::Coordinate(coordinate) => coordinate_id(coordinate),
    }
}

/// The form in which the server takes a position as a place.
fn coordinate_id(coordinate: Coordinate) -> String {
    format!("{},{}", coordinate.latitude, coordinate.longitude)
}

pub(crate) fn parse_trips(response: &HttpResponse) -> Result<Trips, Error> {
    let response: PlanResponse = json(response)?;
    let mut trips = Vec::new();
    for itinerary in response.direct.into_iter().chain(response.itineraries) {
        let legs = itinerary
            .legs
            .into_iter()
            .map(WireLeg::into_leg)
            .collect::<Result<Vec<_>, _>>()?;
        trips.extend(Trip::new(legs));
    }
    Ok(Trips {
        trips,
        earlier: response.previous_page_cursor,
        later: response.next_page_cursor,
    })
}

/// Parses a response. The server answers an unknown stop with status 404
/// and a JSON body, unlike a wrong endpoint.
fn json<'a, T: Deserialize<'a>>(response: &'a HttpResponse) -> Result<T, Error> {
    if response.status == 404
        && let Ok(ErrorBody { error }) = serde_json::from_slice(&response.body)
    {
        return Err(Error::NotFound { message: error });
    }
    response.json()
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
struct PlanResponse {
    /// Trips without public transport, such as a walk all the way.
    #[serde(default)]
    direct: Vec<Itinerary>,
    itineraries: Vec<Itinerary>,
    previous_page_cursor: Option<String>,
    next_page_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Itinerary {
    legs: Vec<WireLeg>,
}

/// What a stop time and a leg of a trip say about the vehicle.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vehicle {
    mode: String,
    /// Whether the times are predictions. Without one the server repeats
    /// the scheduled times in the fields for the predicted ones.
    #[serde(default)]
    real_time: bool,
    headsign: Option<String>,
    agency_name: Option<String>,
    display_name: Option<String>,
    route_short_name: Option<String>,
    #[serde(default)]
    cancelled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopTime {
    place: WirePlace,
    #[serde(flatten)]
    vehicle: Vehicle,
    #[serde(default)]
    trip_cancelled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLeg {
    from: WirePlace,
    to: WirePlace,
    #[serde(flatten)]
    vehicle: Vehicle,
    /// The times of the two ends. The specification requires them here and
    /// not on the places, which carry them too in every recorded response.
    scheduled_start_time: Option<WireTime>,
    scheduled_end_time: Option<WireTime>,
    /// The calls between the two ends.
    #[serde(default)]
    intermediate_stops: Vec<WirePlace>,
}

/// A place, with the times of the call there where it is part of a ride.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePlace {
    /// Missing for places that are not stops, such as the start of a trip
    /// from a coordinate.
    stop_id: Option<String>,
    name: String,
    lat: f64,
    lon: f64,
    tz: Option<String>,
    arrival: Option<WireTime>,
    scheduled_arrival: Option<WireTime>,
    departure: Option<WireTime>,
    scheduled_departure: Option<WireTime>,
    track: Option<String>,
    scheduled_track: Option<String>,
    #[serde(default)]
    cancelled: bool,
}

impl WirePlace {
    fn into_stopover(self, real_time: bool) -> Stopover {
        let predicted = |time: Option<WireTime>| time.filter(|_| real_time).map(|time| time.0);
        let coordinate = Coordinate {
            latitude: self.lat,
            longitude: self.lon,
        };
        Stopover {
            scheduled_arrival: self.scheduled_arrival.map(|time| time.0),
            realtime_arrival: predicted(self.arrival),
            scheduled_departure: self.scheduled_departure.map(|time| time.0),
            realtime_departure: predicted(self.departure),
            scheduled_platform: non_empty(self.scheduled_track),
            realtime_platform: non_empty(self.track),
            cancelled: self.cancelled,
            stop: Location {
                global_ids: self
                    .stop_id
                    .as_deref()
                    .and_then(global_id)
                    .into_iter()
                    .collect(),
                kind: if self.stop_id.is_some() {
                    LocationKind::Stop
                } else {
                    LocationKind::Other
                },
                id: self.stop_id.unwrap_or_else(|| coordinate_id(coordinate)),
                name: self.name,
                locality: None,
                coordinate: Some(coordinate),
                timezone: self.tz,
                network: None,
            },
        }
    }
}

impl Vehicle {
    fn into_line(self, mode: Mode) -> (Line, Option<String>) {
        let line = Line {
            name: non_empty(self.display_name)
                .or(non_empty(self.route_short_name))
                .unwrap_or_default(),
            mode,
            product: None,
            operator: non_empty(self.agency_name),
        };
        (line, non_empty(self.headsign))
    }
}

impl StopTime {
    fn into_departure(self) -> Result<Departure, Error> {
        let cancelled = self.vehicle.cancelled || self.trip_cancelled || self.place.cancelled;
        if self.place.stop_id.is_none() {
            return Err(Error::Parse {
                message: "a stop time has no stop".to_owned(),
            });
        }
        let stopover = self.place.into_stopover(self.vehicle.real_time);
        let mode = transit_mode(&self.vehicle.mode).unwrap_or(Mode::Other);
        let (line, direction) = self.vehicle.into_line(mode);
        Ok(Departure {
            stop: stopover.stop,
            line,
            direction,
            scheduled_time: stopover.scheduled_departure.ok_or_else(|| Error::Parse {
                message: "a stop time has no scheduled departure".to_owned(),
            })?,
            realtime_time: stopover.realtime_departure,
            scheduled_platform: stopover.scheduled_platform,
            realtime_platform: stopover.realtime_platform,
            cancelled,
            remarks: Vec::new(),
        })
    }
}

impl WireLeg {
    fn into_leg(self) -> Result<Leg, Error> {
        let wire = self;
        let no_time = || Error::Parse {
            message: "a leg has no scheduled time at one of its ends".to_owned(),
        };
        let real_time = wire.vehicle.real_time;
        let mut from = wire.from.into_stopover(real_time);
        from.scheduled_departure = from
            .scheduled_departure
            .or(wire.scheduled_start_time.map(|time| time.0));
        let departure = from.into_leg_start().ok_or_else(no_time)?;
        let mut to = wire.to.into_stopover(real_time);
        to.scheduled_arrival = to
            .scheduled_arrival
            .or(wire.scheduled_end_time.map(|time| time.0));
        let arrival = to.into_leg_end().ok_or_else(no_time)?;
        Ok(Leg {
            cancelled: wire.vehicle.cancelled || departure.cancelled || arrival.cancelled,
            departure,
            arrival,
            intermediate_stops: wire
                .intermediate_stops
                .into_iter()
                .map(|place| place.into_stopover(real_time))
                .collect(),
            kind: match wire.vehicle.mode.as_str() {
                "WALK" => LegKind::Walk,
                // The other modes the server lists as street modes.
                "BIKE" | "RENTAL" | "CAR" | "HGV" | "CAR_PARKING" | "CAR_DROPOFF"
                | "RIDE_SHARING" | "FLEX" => LegKind::Other,
                mode => {
                    let mode = transit_mode(mode).unwrap_or(Mode::Other);
                    let (line, direction) = wire.vehicle.into_line(mode);
                    LegKind::Transit { line, direction }
                }
            },
            remarks: Vec::new(),
        })
    }
}

/// The mode of a public transport vehicle. `None` for modes this crate does
/// not know.
fn transit_mode(mode: &str) -> Option<Mode> {
    Some(match mode {
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
        "AERIAL_LIFT" | "AREAL_LIFT" | "CABLE_CAR" => Mode::CableCar,
        "ODM" => Mode::OnDemand,
        _ => return None,
    })
}
