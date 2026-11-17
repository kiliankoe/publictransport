//! The JSON interface (`outputFormat=rapidJSON`) of MDV's EFA journey planner.

use serde::Deserialize;

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, GlobalId, HttpRequest, HttpResponse, Leg,
    LegEnd, LegKind, Line, Location, LocationKind, LocationQuery, Mode, Network, Place, Stopover,
    Timestamp, Trip, TripTime, Trips, TripsQuery, time::WireTime,
};

/// Parameters of every request.
fn common() -> Vec<(&'static str, &'static str)> {
    vec![
        ("outputFormat", "rapidJSON"),
        // The default is a projected coordinate system.
        ("coordOutputFormat", "WGS84[dd.ddddd]"),
    ]
}

pub(crate) fn locations_request(network: &Network, query: &LocationQuery) -> HttpRequest {
    // No limit: `anyMaxSizeHitList` cuts the list before the server rates
    // the matches on some servers, and a vague search then finds nothing.
    // A sum of bits: 1 localities, 2 stops, 4 streets, 8 addresses,
    // 16 crossings, 32 points of interest, 64 postcodes.
    let filter = query
        .kinds
        .iter()
        .fold(0, |bits, kind| {
            bits | match kind {
                LocationKind::Stop => 2,
                LocationKind::Address => 4 | 8,
                LocationKind::Poi => 32,
                LocationKind::Other => 1 | 16 | 64,
            }
        })
        .to_string();
    let mut parameters = common();
    parameters.extend([("type_sf", "any"), ("name_sf", query.text.as_str())]);
    if !query.kinds.is_empty() {
        parameters.push(("anyObjFilter_sf", &filter));
    }
    HttpRequest::get(&network.endpoint, "XML_STOPFINDER_REQUEST", parameters)
}

pub(crate) fn parse_locations(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Location>, Error> {
    // `systemMessages` is not checked: the server reports an ambiguous search
    // as an error there and still returns the matches.
    let mut response: StopFinderResponse = response.json()?;
    // Servers do not promise an order.
    response
        .locations
        .sort_by_key(|location| std::cmp::Reverse(location.match_quality));
    Ok(response
        .locations
        .into_iter()
        .map(|location| location.into_location(network))
        .collect())
}

pub(crate) fn departures_request(
    network: &Network,
    query: &DeparturesQuery,
) -> Result<HttpRequest, Error> {
    let limit = query.limit.map(|limit| limit.to_string());
    let local_time = query
        .time
        .map(|time| local_time(network, time))
        .transpose()?;

    let mut parameters = common();
    parameters.extend([
        ("type_dm", "any"),
        ("name_dm", query.stop_id.as_str()),
        // Skips the page that asks the user to confirm the stop.
        ("mode", "direct"),
        ("useRealtime", "1"),
    ]);
    if let Some(limit) = &limit {
        parameters.push(("limit", limit));
    }
    if let Some((date, time)) = &local_time {
        parameters.extend([
            ("itdDate", date.as_str()),
            ("itdTime", time.as_str()),
            ("itdDateTimeDepArr", "dep"),
        ]);
    }
    Ok(HttpRequest::get(
        &network.endpoint,
        "XML_DM_REQUEST",
        parameters,
    ))
}

pub(crate) fn trips_request(network: &Network, query: &TripsQuery) -> Result<HttpRequest, Error> {
    let limit = query.limit.map(|limit| limit.to_string());
    let (from_type, from) = place(query.origin.checked()?);
    let (to_type, to) = place(query.destination.checked()?);
    let time = query
        .paged_time()?
        .map(|time| match time {
            TripTime::DepartAt(time) => local_time(network, time).map(|local| (local, "dep")),
            TripTime::ArriveBy(time) => local_time(network, time).map(|local| (local, "arr")),
        })
        .transpose()?;

    let mut parameters = common();
    parameters.extend([
        ("type_origin", from_type),
        ("name_origin", from.as_str()),
        ("type_destination", to_type),
        ("name_destination", to.as_str()),
        ("useRealtime", "1"),
        // Without it the server adds a trip from the other side of the time.
        ("calcOneDirection", "1"),
    ]);
    if let Some(limit) = &limit {
        parameters.push(("calcNumberOfTrips", limit));
    }
    if let Some(((date, time), direction)) = &time {
        parameters.extend([
            ("itdDate", date.as_str()),
            ("itdTime", time.as_str()),
            ("itdTripDateTimeDepArr", direction),
        ]);
    }
    Ok(HttpRequest::get(
        &network.endpoint,
        "XML_TRIP_REQUEST2",
        parameters,
    ))
}

/// The walk from one leg to the next. The server does not send it as a leg
/// of its own when it is a change of platform inside a station. `None` if
/// one of the two legs is a walk already or if the walk would not fit
/// between them.
fn walk_between(previous: &Leg, next: &Leg, seconds: i64) -> Option<Leg> {
    let start = previous.arrival.scheduled_time;
    let arrival = Timestamp {
        unix_seconds: start.unix_seconds.checked_add(seconds)?,
    };
    if previous.kind == LegKind::Walk
        || next.kind == LegKind::Walk
        || seconds <= 0
        || arrival > next.departure.scheduled_time
    {
        return None;
    }
    let end = |stop: &Location, scheduled_time| LegEnd {
        stop: stop.clone(),
        scheduled_time,
        realtime_time: None,
        scheduled_platform: None,
        realtime_platform: None,
        cancelled: false,
    };
    Some(Leg {
        kind: LegKind::Walk,
        departure: end(&previous.arrival.stop, start),
        arrival: end(&next.departure.stop, arrival),
        intermediate_stops: Vec::new(),
        cancelled: false,
    })
}

/// The type and name parameters for one end of a trip.
fn place(place: &Place) -> (&'static str, String) {
    match place {
        Place::Id(id) => ("any", id.clone()),
        Place::Coordinate(Coordinate {
            latitude,
            longitude,
        }) => ("coord", format!("{longitude}:{latitude}:WGS84[dd.ddddd]")),
    }
}

/// The date and the time of day as the server wants them: wall-clock time
/// in its own zone.
fn local_time(network: &Network, time: Timestamp) -> Result<(String, String), Error> {
    let zone = network
        .timezone
        .as_deref()
        .ok_or_else(|| Error::InvalidNetwork {
            message: "a request with a time needs the network's time zone".to_owned(),
        })?;
    let zone = jiff::tz::TimeZone::get(zone).map_err(|error| Error::InvalidNetwork {
        message: error.to_string(),
    })?;
    let local = time.to_jiff()?.to_zoned_datetime(zone);
    if !(1..=9999).contains(&local.year()) {
        return Err(Error::InvalidQuery {
            message: "the time is outside the years 1 to 9999".to_owned(),
        });
    }
    Ok((
        local.strftime("%Y%m%d").to_string(),
        local.strftime("%H%M").to_string(),
    ))
}

pub(crate) fn parse_trips(network: &Network, response: &HttpResponse) -> Result<Trips, Error> {
    let response: TripsResponse = response.json()?;
    let Some(journeys) = response.journeys else {
        // Without trips the server explains itself in `systemMessages`.
        // The module that matches places reports one it cannot resolve,
        // e.g. as "origin: multiple matches". Other errors, such as two
        // places too close to each other, mean that there is no trip.
        return match response.system_messages.into_iter().find(|message| {
            message.kind.as_deref() == Some("error") && message.module.as_deref() == Some("BROKER")
        }) {
            Some(message) => Err(Error::NotFound {
                message: message.text.unwrap_or_default(),
            }),
            None => Ok(Trips::paged_by_time(Vec::new())),
        };
    };
    let mut trips = Vec::new();
    for journey in journeys {
        let mut legs: Vec<Leg> = Vec::new();
        // The length of the walk that follows the leg before, in seconds.
        let mut transfer = None;
        for wire in journey.legs {
            let next_transfer = wire.transfer_after();
            let leg = wire.into_leg(network)?;
            if let (Some(seconds), Some(previous)) = (transfer, legs.last())
                && let Some(walk) = walk_between(previous, &leg, seconds)
            {
                legs.push(walk);
            }
            transfer = next_transfer;
            legs.push(leg);
        }
        if !legs.is_empty() {
            trips.push(Trip { legs });
        }
    }
    Ok(Trips::paged_by_time(trips))
}

pub(crate) fn parse_departures(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Departure>, Error> {
    let response: DeparturesResponse = response.json()?;
    // Unless the query names exactly one stop, the server sends the places
    // it considered and no departures.
    let Some(stop_events) = response.stop_events else {
        return match response.locations.len() {
            1 => Ok(Vec::new()),
            candidates => Err(Error::NotFound {
                message: format!("the server matched {candidates} places to the stop"),
            }),
        };
    };
    let mut departures: Vec<_> = stop_events
        .into_iter()
        .map(|event| event.into_departure(network))
        .collect();
    departures.sort_by_key(Departure::expected_time);
    Ok(departures)
}

#[derive(Deserialize)]
struct StopFinderResponse {
    locations: Vec<WireLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeparturesResponse {
    /// Tells an EFA response from other JSON.
    #[serde(rename = "version")]
    _version: String,
    /// The stop the server resolved the query to, or its candidates.
    #[serde(default)]
    locations: Vec<WireLocation>,
    stop_events: Option<Vec<StopEvent>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TripsResponse {
    /// Tells an EFA response from other JSON.
    #[serde(rename = "version")]
    _version: String,
    #[serde(default)]
    system_messages: Vec<SystemMessage>,
    journeys: Option<Vec<Journey>>,
}

#[derive(Deserialize)]
struct SystemMessage {
    #[serde(rename = "type")]
    kind: Option<String>,
    module: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct Journey {
    legs: Vec<WireLeg>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLeg {
    origin: WireLocation,
    destination: WireLocation,
    transportation: Transportation,
    /// Every call of the ride, including the two ends.
    #[serde(default)]
    stop_sequence: Vec<WireLocation>,
    is_cancelled: Option<bool>,
    #[serde(default)]
    realtime_status: Vec<String>,
    /// Without this the estimated times at the two ends only repeat the
    /// planned ones or carry on the delay of the leg before.
    #[serde(default)]
    is_realtime_controlled: bool,
    #[serde(default)]
    foot_path_info: Vec<FootPath>,
}

/// A walk that belongs to a leg.
#[derive(Deserialize)]
struct FootPath {
    /// "AFTER" for the walk from the end of the leg to the next one.
    position: Option<String>,
    /// In seconds.
    duration: Option<i64>,
}

/// A place. Inside a trip it also carries the times of the call there.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLocation {
    id: String,
    is_global_id: Option<bool>,
    /// Includes the locality, e.g. "Dresden, Hauptbahnhof".
    name: String,
    /// For a platform this is only the platform's number.
    disassembled_name: Option<String>,
    /// Latitude, then longitude.
    coord: Option<[f64; 2]>,
    #[serde(rename = "type")]
    kind: String,
    parent: Option<Ancestor>,
    #[serde(default)]
    match_quality: i64,
    properties: Option<Properties>,
    arrival_time_planned: Option<WireTime>,
    arrival_time_estimated: Option<WireTime>,
    departure_time_planned: Option<WireTime>,
    departure_time_estimated: Option<WireTime>,
}

/// The locality of a stop, or the stop of a platform.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ancestor {
    name: Option<String>,
    disassembled_name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    parent: Option<Box<Ancestor>>,
}

impl WireLocation {
    /// The closest ancestor of the given type.
    fn ancestor(&self, kind: &str) -> Option<&Ancestor> {
        std::iter::successors(self.parent.as_ref(), |ancestor| ancestor.parent.as_deref())
            .find(|ancestor| ancestor.kind.as_deref() == Some(kind))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    platform_name: Option<String>,
    planned_platform_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopEvent {
    location: WireLocation,
    departure_time_planned: WireTime,
    departure_time_estimated: Option<WireTime>,
    transportation: Transportation,
    is_cancelled: Option<bool>,
    #[serde(default)]
    realtime_status: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transportation {
    /// The product and the number, e.g. "Straßenbahn 7".
    name: Option<String>,
    number: Option<String>,
    properties: Option<TransportationProperties>,
    product: Option<Product>,
    operator: Option<Named>,
    destination: Option<Named>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransportationProperties {
    /// The train category, e.g. "ICE". Only meaningful for trains: servers
    /// fill it with unrelated values for buses and trams.
    train_type: Option<String>,
    /// "TRAIN" for trains that have a number but no line.
    line_display: Option<String>,
}

#[derive(Deserialize)]
struct Product {
    class: Option<u32>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct Named {
    name: Option<String>,
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
}

impl WireLocation {
    fn into_location(self, network: &Network) -> Location {
        let wire = self;
        let locality = wire
            .ancestor("locality")
            .and_then(|locality| locality.name.clone());
        let (kind, name) = match wire.kind.as_str() {
            // A platform's own name is only its number.
            "platform" => (
                LocationKind::Stop,
                wire.ancestor("stop").and_then(|stop| {
                    non_empty(stop.disassembled_name.clone()).or_else(|| stop.name.clone())
                }),
            ),
            "stop" => (LocationKind::Stop, None),
            "street" | "singlehouse" => (LocationKind::Address, None),
            "poi" => (LocationKind::Poi, None),
            _ => (LocationKind::Other, None),
        };
        Location {
            // The server sends no time zone.
            timezone: network.timezone.clone(),
            network: None,
            global_ids: GlobalId::ifopt(&wire.id)
                .filter(|_| wire.is_global_id == Some(true))
                .into_iter()
                .collect(),
            id: wire.id,
            kind,
            name: name
                .or(non_empty(wire.disassembled_name))
                .unwrap_or(wire.name),
            locality,
            coordinate: wire.coord.map(|[latitude, longitude]| Coordinate {
                latitude,
                longitude,
            }),
        }
    }
}

impl WireLocation {
    fn into_stopover(mut self, network: &Network) -> Stopover {
        let platforms = self.properties.take();
        Stopover {
            scheduled_arrival: self.arrival_time_planned.take().map(|time| time.0),
            realtime_arrival: self.arrival_time_estimated.take().map(|time| time.0),
            scheduled_departure: self.departure_time_planned.take().map(|time| time.0),
            realtime_departure: self.departure_time_estimated.take().map(|time| time.0),
            scheduled_platform: platforms
                .as_ref()
                .and_then(|platforms| non_empty(platforms.planned_platform_name.clone())),
            realtime_platform: platforms.and_then(|platforms| non_empty(platforms.platform_name)),
            cancelled: false,
            stop: self.into_location(network),
        }
    }
}

/// Unconfirmed: no recorded response contains a cancellation. Other clients
/// read these two fields.
fn is_cancelled(is_cancelled: Option<bool>, realtime_status: &[String]) -> bool {
    is_cancelled == Some(true)
        || realtime_status
            .iter()
            .any(|status| status == "TRIP_CANCELLED")
}

impl Transportation {
    /// How a leg with this transportation is traveled, if not by public
    /// transport. Classes 99 and 100 are footpaths. The other classes from
    /// 97 on stand for bikes, cars and the like according to other clients;
    /// no recorded response contains one.
    fn street_kind(&self) -> Option<LegKind> {
        match self.product.as_ref().and_then(|product| product.class) {
            Some(99 | 100) => Some(LegKind::Walk),
            Some(97..) | None => Some(LegKind::Other),
            Some(_) => None,
        }
    }

    /// The line and the direction of the vehicle.
    fn into_line(self) -> (Line, Option<String>) {
        let train_type = self
            .properties
            .as_ref()
            .and_then(|properties| non_empty(properties.train_type.clone()));
        let has_line = self
            .properties
            .and_then(|properties| properties.line_display)
            .is_none_or(|display| display != "TRAIN");
        let number = non_empty(self.number);
        let line = Line {
            name: match (has_line, &train_type, number) {
                (false, Some(train_type), Some(number)) => format!("{train_type} {number}"),
                (_, _, number) => number.or(self.name).unwrap_or_default(),
            },
            mode: mode(
                self.product.as_ref().and_then(|product| product.class),
                has_line,
                train_type.as_deref(),
            ),
            product: self.product.and_then(|product| non_empty(product.name)),
            operator: self.operator.and_then(|operator| non_empty(operator.name)),
        };
        let direction = self
            .destination
            .and_then(|destination| non_empty(destination.name));
        (line, direction)
    }
}

impl StopEvent {
    fn into_departure(self, network: &Network) -> Departure {
        let (line, direction) = self.transportation.into_line();
        let mut location = self.location;
        let platforms = location.properties.take();
        Departure {
            stop: location.into_location(network),
            line,
            direction,
            scheduled_time: self.departure_time_planned.0,
            realtime_time: self.departure_time_estimated.map(|time| time.0),
            scheduled_platform: platforms
                .as_ref()
                .and_then(|platforms| non_empty(platforms.planned_platform_name.clone())),
            realtime_platform: platforms.and_then(|platforms| non_empty(platforms.platform_name)),
            cancelled: is_cancelled(self.is_cancelled, &self.realtime_status),
        }
    }
}

impl WireLeg {
    /// The length in seconds of the walk that follows this leg.
    fn transfer_after(&self) -> Option<i64> {
        self.foot_path_info
            .iter()
            .filter(|path| path.position.as_deref() == Some("AFTER"))
            .find_map(|path| path.duration)
    }

    fn into_leg(self, network: &Network) -> Result<Leg, Error> {
        let no_time = || Error::Parse {
            message: "a leg has no scheduled time at one of its ends".to_owned(),
        };
        let kind = self.transportation.street_kind().unwrap_or_else(|| {
            let (line, direction) = self.transportation.into_line();
            LegKind::Transit { line, direction }
        });
        let mut departure = self
            .origin
            .into_stopover(network)
            .into_leg_start()
            .ok_or_else(no_time)?;
        let mut arrival = self
            .destination
            .into_stopover(network)
            .into_leg_end()
            .ok_or_else(no_time)?;
        if !self.is_realtime_controlled {
            departure.realtime_time = None;
            arrival.realtime_time = None;
        }
        let between = self.stop_sequence.len().saturating_sub(2);
        Ok(Leg {
            kind,
            departure,
            arrival,
            intermediate_stops: self
                .stop_sequence
                .into_iter()
                .skip(1)
                .take(between)
                .map(|call| call.into_stopover(network))
                .collect(),
            cancelled: is_cancelled(self.is_cancelled, &self.realtime_status),
        })
    }
}

/// Maps a product class ("Verkehrsmittelklasse"). The numbers are from other
/// clients and the product names servers send with them, not from a
/// specification.
fn mode(class: Option<u32>, has_line: bool, train_type: Option<&str>) -> Mode {
    match class {
        // Some servers, such as VVO's, put every train into class 0. Trains
        // without a line are the long-distance ones there.
        Some(0) if !has_line => match train_type {
            Some("ICE" | "RJ" | "RJX" | "TGV") => Mode::HighSpeedTrain,
            Some("EN" | "NJ") => Mode::NightTrain,
            _ => Mode::LongDistanceTrain,
        },
        Some(0 | 13) => Mode::RegionalTrain,
        Some(1) => Mode::SuburbanTrain,
        Some(2) => Mode::Subway,
        Some(3 | 4) => Mode::Tram,
        Some(5 | 6 | 7 | 17 | 19) => Mode::Bus,
        Some(8) => Mode::CableCar,
        Some(9) => Mode::Ferry,
        Some(10) => Mode::OnDemand,
        Some(14 | 15) => Mode::LongDistanceTrain,
        Some(16) => Mode::HighSpeedTrain,
        _ => Mode::Other,
    }
}
