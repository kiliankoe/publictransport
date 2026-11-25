//! The JSON interface (`outputFormat=rapidJSON`) of MDV's EFA journey planner.

use serde::Deserialize;

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, GlobalId, HttpRequest, HttpResponse, Journey,
    Leg, LegEnd, LegKind, Line, Location, LocationKind, LocationQuery, Mode, NearbyQuery, Network,
    Remark, Stopover, Timestamp, Trip, TripTime, Trips, TripsQuery,
    http::{lenient_list, lenient_text},
    model::{Board, Target, journey_id, journey_parts, plain_text},
    time::WireTime,
};

/// Parameters of every request.
fn common(network: &Network) -> Vec<(&'static str, &str)> {
    let mut parameters = vec![
        ("outputFormat", "rapidJSON"),
        // The default is a projected coordinate system.
        ("coordOutputFormat", "WGS84[dd.ddddd]"),
    ];
    parameters.extend(network.language_as("language"));
    parameters
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
    let mut parameters = common(network);
    parameters.extend([("type_sf", "any"), ("name_sf", query.text.as_str())]);
    if !query.kinds.is_empty() {
        parameters.push(("anyObjFilter_sf", &filter));
    }
    HttpRequest::get(&network.endpoint, "XML_STOPFINDER_REQUEST", parameters)
}

/// The stops within a circle, which the server orders by distance.
pub(crate) fn nearby_request(network: &Network, query: &NearbyQuery) -> Result<HttpRequest, Error> {
    let position = query.position.checked()?;
    let center = format!(
        "{}:{}:WGS84[dd.ddddd]",
        position.longitude, position.latitude
    );
    let radius = query.mercator_radius().to_string();
    let limit = query.limit.map(|limit| limit.to_string());
    let mut parameters = common(network);
    parameters.extend([
        ("coord", center.as_str()),
        ("inclFilter", "1"),
        ("type_1", "STOP"),
        ("radius_1", radius.as_str()),
    ]);
    if let Some(limit) = &limit {
        parameters.push(("max", limit));
    }
    Ok(HttpRequest::get(
        &network.endpoint,
        "XML_COORD_REQUEST",
        parameters,
    ))
}

pub(crate) fn parse_nearby(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Location>, Error> {
    let response: StopFinderResponse = response.json()?;
    Ok(response
        .locations
        .into_iter()
        .map(|location| location.into_location(network))
        .collect())
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
    board: Board,
) -> Result<HttpRequest, Error> {
    let stop = query.stop.stop_id(network)?;
    let limit = query.limit.map(|limit| limit.to_string());
    let local_time = query
        .time
        .map(|time| local_time(network, time))
        .transpose()?;

    let mut parameters = common(network);
    parameters.extend([
        ("type_dm", "any"),
        ("name_dm", stop.as_str()),
        // Skips the page that asks the user to confirm the stop.
        ("mode", "direct"),
        ("useRealtime", "1"),
    ]);
    if let Some(limit) = &limit {
        parameters.push(("limit", limit));
    }
    if let Some((date, time)) = &local_time {
        parameters.extend([("itdDate", date.as_str()), ("itdTime", time.as_str())]);
    }
    // Without it the server lists departures, also for a time.
    match board {
        Board::Arrivals => parameters.push(("itdDateTimeDepArr", "arr")),
        Board::Departures if local_time.is_some() => {
            parameters.push(("itdDateTimeDepArr", "dep"));
        }
        Board::Departures => {}
    }
    Ok(HttpRequest::get(
        &network.endpoint,
        "XML_DM_REQUEST",
        parameters,
    ))
}

pub(crate) fn trips_request(network: &Network, query: &TripsQuery) -> Result<HttpRequest, Error> {
    let limit = query.limit.map(|limit| limit.to_string());
    let (from_type, from) = place(query.origin.target(network)?);
    let (to_type, to) = place(query.destination.target(network)?);
    let time = query
        .paged_time()?
        .map(|time| match time {
            TripTime::DepartAt(time) => local_time(network, time).map(|local| (local, "dep")),
            TripTime::ArriveBy(time) => local_time(network, time).map(|local| (local, "arr")),
        })
        .transpose()?;

    let mut parameters = common(network);
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

/// The server finds a run by its line, its trip code and one of its calls:
/// a stop and the planned time there. The ID holds these four.
pub(crate) fn journey_request(network: &Network, id: &str) -> Result<HttpRequest, Error> {
    let (line, stop, trip_code, time): (String, String, String, i64) = journey_parts(id)?;
    let (date, time) = local_time(network, Timestamp { unix_seconds: time })?;
    let mut parameters = common(network);
    parameters.extend([
        ("line", line.as_str()),
        ("stopID", stop.as_str()),
        ("tripCode", trip_code.as_str()),
        ("date", date.as_str()),
        ("time", time.as_str()),
        // Every stop of the run and not only those from `stopID` on.
        ("tStOTType", "ALL"),
        ("useRealtime", "1"),
    ]);
    Ok(HttpRequest::get(
        &network.endpoint,
        "XML_TRIPSTOPTIMES_REQUEST",
        parameters,
    ))
}

pub(crate) fn parse_journey(network: &Network, response: &HttpResponse) -> Result<Journey, Error> {
    let response: JourneyResponse = response.json()?;
    // For a run it does not know, the server sends the line without stops.
    let leg = response.leg.ok_or_else(Journey::unknown)?;
    let (line, direction) = leg.transportation.into_line();
    Ok(Journey {
        line,
        direction,
        stops: leg
            .stop_sequence
            .into_iter()
            .map(|call| call.into_stopover(network))
            .collect(),
        cancelled: is_cancelled(leg.is_cancelled, &leg.realtime_status),
        remarks: remarks(network, leg.infos, leg.hints),
    })
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
    // The walk starts when the leg before it ends, late or not.
    let delay = previous.arrival.delay_seconds();
    let end = |stop: &Location, scheduled_time: Timestamp| LegEnd {
        stop: stop.clone(),
        scheduled_time,
        realtime_time: delay.map(|delay| Timestamp {
            unix_seconds: scheduled_time.unix_seconds.saturating_add(delay),
        }),
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
        remarks: Vec::new(),
        journey_id: None,
    })
}

/// The type and name parameters for one end of a trip.
fn place(place: Target) -> (&'static str, String) {
    match place {
        Target::Id(id) => ("any", id),
        Target::Coordinate(Coordinate {
            latitude,
            longitude,
        }) => ("coord", format!("{longitude}:{latitude}:WGS84[dd.ddddd]")),
    }
}

/// The date and the time of day as the server wants them: wall-clock time
/// in its own zone.
fn local_time(network: &Network, time: Timestamp) -> Result<(String, String), Error> {
    let local = network.local(time)?;
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
        // The server estimates times for a walk too: it carries the delay
        // of the ride before it on. Without a ride that has real-time data,
        // these are the planned times again and no prediction.
        let trip_is_live = journey.legs.iter().any(|leg| leg.is_realtime_controlled);
        for wire in journey.legs {
            let next_transfer = wire.transfer_after();
            let leg = wire.into_leg(network, trip_is_live)?;
            if let (Some(seconds), Some(previous)) = (transfer, legs.last())
                && let Some(walk) = walk_between(previous, &leg, seconds)
            {
                legs.push(walk);
            }
            transfer = next_transfer;
            legs.push(leg);
        }
        trips.extend(Trip::new(legs));
    }
    Ok(Trips::paged_by_time(trips))
}

pub(crate) fn parse_departures(
    network: &Network,
    response: &HttpResponse,
    board: Board,
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
    let mut departures = stop_events
        .into_iter()
        .map(|event| event.into_departure(network, board))
        .collect::<Result<Vec<_>, _>>()?;
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
    journeys: Option<Vec<WireTrip>>,
}

#[derive(Deserialize)]
struct JourneyResponse {
    /// Tells an EFA response from other JSON.
    #[serde(rename = "version")]
    _version: String,
    leg: Option<JourneyLeg>,
}

/// A run: a leg of a trip without its two ends.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JourneyLeg {
    transportation: Transportation,
    #[serde(default)]
    stop_sequence: Vec<WireLocation>,
    is_cancelled: Option<bool>,
    #[serde(default)]
    realtime_status: Vec<String>,
    #[serde(default, deserialize_with = "lenient_list")]
    infos: Vec<Info>,
    #[serde(default, deserialize_with = "lenient_list")]
    hints: Vec<Hint>,
}

#[derive(Deserialize)]
struct SystemMessage {
    #[serde(rename = "type")]
    kind: Option<String>,
    module: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct WireTrip {
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
    /// Messages of the incident system.
    #[serde(default, deserialize_with = "lenient_list")]
    infos: Vec<Info>,
    /// Properties of the ride.
    #[serde(default, deserialize_with = "lenient_list")]
    hints: Vec<Hint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    id: Option<String>,
    #[serde(default, rename = "infoLinks")]
    links: Vec<InfoLink>,
    properties: Option<InfoProperties>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InfoLink {
    title: Option<String>,
    subtitle: Option<String>,
    url_text: Option<String>,
    url: Option<String>,
    /// The message, often as HTML.
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InfoProperties {
    /// Two local times, as in `16/11/2026 08:00 - 20/11/2026 14:00`.
    incident_date_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hint {
    content: Option<String>,
    provider_code: Option<String>,
}

/// The remarks of a stop event or a leg.
fn remarks(network: &Network, infos: Vec<Info>, hints: Vec<Hint>) -> Vec<Remark> {
    let plain = |text: Option<String>| text.as_deref().and_then(plain_text);
    let disruptions = infos.into_iter().filter_map(|info| {
        let link = info.links.into_iter().next()?;
        let (from, until) = info
            .properties
            .and_then(|properties| properties.incident_date_time)
            .and_then(|period| validity(network, &period))
            .unzip();
        let title = match (plain(link.title), plain(link.subtitle)) {
            (Some(title), Some(subtitle)) if subtitle != title => {
                Some(format!("{title} {subtitle}"))
            }
            (title, subtitle) => title.or(subtitle).or_else(|| plain(link.url_text)),
        };
        Remark {
            title,
            text: plain(link.content),
            url: link.url.filter(|url| opens(url)),
            id: non_empty(info.id),
            valid_from: from,
            valid_until: until,
            ..Remark::disruption()
        }
        .if_any()
    });
    let hints = hints
        .into_iter()
        .filter_map(|hint| Some(Remark::hint(plain(hint.content)?, hint.provider_code)));
    disruptions.chain(hints).collect()
}

/// The two times of a period that the server writes on the clocks of the
/// network.
fn validity(network: &Network, period: &str) -> Option<(Timestamp, Timestamp)> {
    let zone = network.zone().ok()?;
    let time = |text: &str| {
        let local = jiff::civil::DateTime::strptime("%d/%m/%Y %H:%M", text.trim()).ok()?;
        Some(Timestamp {
            unix_seconds: zone
                .to_ambiguous_timestamp(local)
                .compatible()
                .ok()?
                .as_second(),
        })
    };
    let (from, until) = period.split_once(" - ")?;
    Some((time(from)?, time(until)?))
}

/// Whether somebody outside the operator's network can open the URL. Some
/// servers send a path, no host, a made-up one such as `nohost`, or an
/// address of a private network.
fn opens(url: &str) -> bool {
    let Some(rest) = ["https://", "http://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
    else {
        return false;
    };
    let host = rest.split(['/', ':', '?']).next().unwrap_or_default();
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(address) => !(address.is_private() || address.is_loopback() || address.is_link_local()),
        Err(_) => host.contains('.'),
    }
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
    #[serde(default, deserialize_with = "lenient_text")]
    id: Option<String>,
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
    /// An answer has the times of the side that was asked for.
    departure_time_planned: Option<WireTime>,
    departure_time_estimated: Option<WireTime>,
    arrival_time_planned: Option<WireTime>,
    arrival_time_estimated: Option<WireTime>,
    transportation: Transportation,
    is_cancelled: Option<bool>,
    #[serde(default)]
    realtime_status: Vec<String>,
    /// Messages of the incident system.
    #[serde(default, deserialize_with = "lenient_list")]
    infos: Vec<Info>,
    /// Properties of the ride.
    #[serde(default, deserialize_with = "lenient_list")]
    hints: Vec<Hint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transportation {
    #[serde(default, deserialize_with = "lenient_text")]
    id: Option<String>,
    /// The product and the number, e.g. "Straßenbahn 7".
    name: Option<String>,
    number: Option<String>,
    properties: Option<TransportationProperties>,
    product: Option<Product>,
    operator: Option<Named>,
    origin: Option<Named>,
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
    /// Tells the runs of a line apart.
    #[serde(default, deserialize_with = "lenient_text")]
    trip_code: Option<String>,
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
            .and_then(|locality| non_empty(locality.name.clone()));
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

/// The ID of the run of a vehicle that calls at a place at a planned time.
/// `None` for a walk and if the server leaves out a part.
fn run_id(vehicle: &Transportation, place: &WireLocation, planned: Timestamp) -> Option<String> {
    let trip_code = vehicle.properties.as_ref()?.trip_code.as_ref()?;
    // The server knows the run by the stop, not by the platform.
    let stop = place
        .ancestor("stop")
        .and_then(|stop| stop.id.as_ref())
        .unwrap_or(&place.id);
    let line = non_empty(vehicle.id.clone())?;
    journey_id(&(line, stop, trip_code, planned.unix_seconds))
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
    fn into_departure(mut self, network: &Network, board: Board) -> Result<Departure, Error> {
        let origin = self.transportation.origin.take();
        // The server finds a run by an arrival time as well.
        let journey_id = match board {
            Board::Departures => self.departure_time_planned.as_ref(),
            Board::Arrivals => self.arrival_time_planned.as_ref(),
        }
        .and_then(|planned| run_id(&self.transportation, &self.location, planned.0));
        let (line, direction) = self.transportation.into_line();
        let (planned, estimated, direction) = match board {
            Board::Departures => (
                self.departure_time_planned,
                self.departure_time_estimated,
                direction,
            ),
            Board::Arrivals => (
                self.arrival_time_planned,
                self.arrival_time_estimated,
                origin.and_then(|origin| non_empty(origin.name)),
            ),
        };
        let mut location = self.location;
        let platforms = location.properties.take();
        Ok(Departure {
            stop: location.into_location(network),
            line,
            direction,
            scheduled_time: planned
                .ok_or_else(|| Error::Parse {
                    message: "a stop event has no scheduled time".to_owned(),
                })?
                .0,
            realtime_time: estimated.map(|time| time.0),
            scheduled_platform: platforms
                .as_ref()
                .and_then(|platforms| non_empty(platforms.planned_platform_name.clone())),
            realtime_platform: platforms.and_then(|platforms| non_empty(platforms.platform_name)),
            cancelled: is_cancelled(self.is_cancelled, &self.realtime_status),
            remarks: remarks(network, self.infos, self.hints),
            journey_id,
        })
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

    /// `trip_is_live` says whether a ride of the trip has real-time data, which the
    /// estimated times of a walk then follow.
    fn into_leg(self, network: &Network, trip_is_live: bool) -> Result<Leg, Error> {
        let no_time = || Error::Parse {
            message: "a leg has no scheduled time at one of its ends".to_owned(),
        };
        let street_kind = self.transportation.street_kind();
        let journey_id = self
            .origin
            .departure_time_planned
            .as_ref()
            .filter(|_| street_kind.is_none())
            .and_then(|planned| run_id(&self.transportation, &self.origin, planned.0));
        let predicted = self.is_realtime_controlled || (trip_is_live && street_kind.is_some());
        let kind = street_kind.unwrap_or_else(|| {
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
        if !predicted {
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
            remarks: remarks(network, self.infos, self.hints),
            journey_id,
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
            // South Tyrol's regional trains have a number and no line.
            Some("R" | "RV" | "REG") => Mode::RegionalTrain,
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
