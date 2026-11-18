//! `OpenTripPlanner` 2 with its Transmodel API: one `POST` of a GraphQL
//! query per request. OTP has no search for stops by name worth using, so
//! location searches go to a geocoder in the Pelias format.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, Header, HttpRequest, HttpResponse, Leg, LegEnd,
    LegKind, Line, Location, LocationKind, LocationQuery, Method, Mode, Network, Place, Stopover,
    Timestamp, Trip, TripTime, Trips, TripsQuery, model::Target, time::WireTime,
};

// The queries name only fields that OTP has had for long: a server answers
// a query with a field it does not know with an error and no data.
const QUAY: &str =
    "fragment quay on Quay{id name publicCode latitude longitude timeZone stopPlace{name}}";
const LINE: &str =
    "fragment line on Line{publicCode name transportMode transportSubmode authority{name}}";
const CALL: &str = "fragment call on EstimatedCall{aimedArrivalTime expectedArrivalTime \
    aimedDepartureTime expectedDepartureTime realtime cancellation \
    destinationDisplay{frontText} quay{...quay} scheduledQuay{publicCode} \
    serviceJourney{line{...line}}}";
const CALLS: &str = "estimatedCalls(numberOfDepartures:$limit,startTime:$start,\
    includeCancelledTrips:true){...call}";
const TRIPS: &str = "query($from:Location!,$to:Location!,$time:DateTime,$arriveBy:Boolean,\
    $limit:Int,$page:String){trip(from:$from,to:$to,dateTime:$time,arriveBy:$arriveBy,\
    numTripPatterns:$limit,pageCursor:$page){previousPageCursor nextPageCursor routingErrors{code} \
    tripPatterns{legs{mode aimedStartTime expectedStartTime aimedEndTime expectedEndTime \
    realtime fromPlace{...place} toPlace{...place} line{...line} \
    fromEstimatedCall{...call} toEstimatedCall{...call} \
    intermediateEstimatedCalls{...call}}}}} \
    fragment place on Place{name latitude longitude quay{...quay}}";

/// A request with a GraphQL query. A variable without a value is left
/// out, so that the server's default applies.
fn request<const N: usize>(
    network: &Network,
    query: &str,
    variables: [(&str, Option<Value>); N],
) -> HttpRequest {
    let variables: serde_json::Map<_, _> = variables
        .into_iter()
        .filter_map(|(name, value)| Some((name.to_owned(), value?)))
        .collect();
    let body = json!({"query": format!("{query} {CALL} {QUAY} {LINE}"), "variables": variables});
    HttpRequest {
        method: Method::Post,
        url: network.endpoint.trim_end_matches('/').to_owned(),
        headers: vec![Header {
            name: "Content-Type".to_owned(),
            value: "application/json".to_owned(),
        }],
        body: Some(body.to_string().into_bytes()),
    }
}

/// A time as the server takes it.
fn time(time: Timestamp) -> Result<Value, Error> {
    Ok(json!(time.to_jiff()?.to_string()))
}

pub(crate) fn locations_request(
    network: &Network,
    query: &LocationQuery,
) -> Result<HttpRequest, Error> {
    let geocoder = network
        .geocoder
        .as_deref()
        .ok_or_else(|| Error::Unsupported {
            message: format!(
                "{} has no geocoder, and its server has no search for locations",
                network.id
            ),
        })?;
    // The geocoder has addresses and points of interest in one layer.
    let layers: Vec<&str> = [
        ("venue", &[LocationKind::Stop][..]),
        ("address", &[LocationKind::Address, LocationKind::Poi]),
    ]
    .into_iter()
    .filter(|(_, kinds)| kinds.iter().any(|kind| query.kinds.contains(kind)))
    .map(|(layer, _)| layer)
    .collect();
    if layers.is_empty() && !query.kinds.is_empty() {
        return Err(Error::InvalidQuery {
            message: "the geocoder cannot search for the asked kinds of location alone".to_owned(),
        });
    }
    let limit = query.limit.map(|limit| limit.to_string());
    let layers = layers.join(",");
    let mut parameters = vec![("text", query.text.as_str())];
    if let Some(limit) = &limit {
        parameters.push(("size", limit));
    }
    if !layers.is_empty() {
        parameters.push(("layers", &layers));
    }
    Ok(HttpRequest::get(geocoder, "autocomplete", parameters))
}

pub(crate) fn departures_request(
    network: &Network,
    query: &DeparturesQuery,
) -> Result<HttpRequest, Error> {
    // The IDs of the Nordic stop registries name the kind of place.
    let stop = query.stop.stop_id(network)?;
    let place = if stop.contains(":Quay:") {
        "quay"
    } else {
        "stopPlace"
    };
    let text =
        format!("query($id:String!,$limit:Int,$start:DateTime){{{place}(id:$id){{{CALLS}}}}}");
    Ok(request(
        network,
        &text,
        [
            ("id", Some(json!(stop))),
            ("limit", query.limit.map(|limit| json!(limit))),
            ("start", query.time.map(time).transpose()?),
        ],
    ))
}

pub(crate) fn trips_request(network: &Network, query: &TripsQuery) -> Result<HttpRequest, Error> {
    let place = |place: &Place| {
        Ok::<_, Error>(match place.target(network)? {
            Target::Id(id) => json!({"place": id}),
            Target::Coordinate(coordinate) => json!({"coordinates": {
                "latitude": coordinate.latitude,
                "longitude": coordinate.longitude,
            }}),
        })
    };
    let (asked, arrive_by) = match query.time {
        Some(TripTime::DepartAt(asked)) => (Some(asked), false),
        Some(TripTime::ArriveBy(asked)) => (Some(asked), true),
        None => (None, false),
    };
    Ok(request(
        network,
        TRIPS,
        [
            ("from", Some(place(&query.origin)?)),
            ("to", Some(place(&query.destination)?)),
            ("time", asked.map(time).transpose()?),
            ("arriveBy", arrive_by.then_some(json!(true))),
            ("limit", query.limit.map(|limit| json!(limit))),
            // The server continues the search of the reference, on top of
            // the request it answered.
            ("page", query.page.as_ref().map(|page| json!(page))),
        ],
    ))
}

pub(crate) fn parse_locations(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Location>, Error> {
    let found: Features = response.json()?;
    Ok(found
        .features
        .into_iter()
        .map(|feature| {
            let properties = feature.properties;
            let is = |category: &str| properties.category.iter().any(|name| name == category);
            Location {
                kind: if properties.layer.as_deref() == Some("venue") {
                    LocationKind::Stop
                } else if is("poi") {
                    LocationKind::Poi
                } else if is("GroupOfStopPlaces") {
                    LocationKind::Other
                } else {
                    LocationKind::Address
                },
                id: properties.id.unwrap_or_default(),
                global_ids: Vec::new(),
                name: properties.name.unwrap_or_default(),
                locality: properties.locality,
                // Longitude first, as in GeoJSON.
                coordinate: match feature.geometry.coordinates.as_slice() {
                    [longitude, latitude, ..] => Some(Coordinate {
                        latitude: *latitude,
                        longitude: *longitude,
                    }),
                    _ => None,
                },
                timezone: network.timezone.clone(),
                network: None,
            }
        })
        .collect())
}

/// The data of a GraphQL answer, and the first error the server reports
/// next to it. An answer can have both: the error then explains what is
/// missing in the data.
fn data(response: &HttpResponse) -> Result<(Data, Option<Error>), Error> {
    let answer: Answer = response.json()?;
    let error = answer
        .errors
        .into_iter()
        .flatten()
        .next()
        .map(|error| Error::Server {
            status: 200,
            message: error.message.unwrap_or_default(),
        });
    match (answer.data, error) {
        (Some(data), error) => Ok((data, error)),
        (None, Some(error)) => Err(error),
        (None, None) => Err(Error::Parse {
            message: "the response has no data".to_owned(),
        }),
    }
}

/// What a result without entries stands for: the error the server
/// reported next to it, if any.
fn explained<T>(entries: Vec<T>, error: Option<Error>) -> Result<Vec<T>, Error> {
    match error {
        Some(error) if entries.is_empty() => Err(error),
        _ => Ok(entries),
    }
}

pub(crate) fn parse_departures(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Departure>, Error> {
    let (data, error) = data(response)?;
    let Some(place) = data.stop_place.or(data.quay) else {
        return Err(error.unwrap_or_else(|| Error::NotFound {
            message: "the server does not know the stop".to_owned(),
        }));
    };
    let mut departures = explained(place.estimated_calls, error)?
        .into_iter()
        .map(|mut call| {
            let line = call.service_journey.take().and_then(|journey| journey.line);
            let direction = call.direction();
            let stopover = call.into_stopover(network)?;
            let incomplete = || Error::Parse {
                message: "a departure has no line or no time".to_owned(),
            };
            Ok(Departure {
                line: line.map(WireLine::into_line).ok_or_else(incomplete)?,
                direction,
                stop: stopover.stop,
                scheduled_time: stopover.scheduled_departure.ok_or_else(incomplete)?,
                realtime_time: stopover.realtime_departure,
                scheduled_platform: stopover.scheduled_platform,
                realtime_platform: stopover.realtime_platform,
                cancelled: stopover.cancelled,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    departures.sort_by_key(Departure::expected_time);
    Ok(departures)
}

pub(crate) fn parse_trips(network: &Network, response: &HttpResponse) -> Result<Trips, Error> {
    let (data, error) = data(response)?;
    let Some(trip) = data.trip else {
        return Err(error.unwrap_or_else(|| Error::Parse {
            message: "the response has no trips".to_owned(),
        }));
    };
    // The other codes say why a search found nothing, as when the window
    // of time it looked at has no trip.
    if let Some(unknown) = trip
        .routing_errors
        .iter()
        .find(|error| error.code.as_deref() == Some("locationNotFound"))
    {
        return Err(Error::NotFound {
            message: unknown.code.clone().unwrap_or_default(),
        });
    }
    let trips = explained(trip.trip_patterns, error)?
        .into_iter()
        .map(|pattern| {
            let legs = pattern
                .legs
                .into_iter()
                .map(|leg| leg.into_leg(network))
                .collect::<Result<_, _>>()?;
            Ok(Trip { legs })
        })
        .collect::<Result<_, Error>>()?;
    Ok(Trips {
        trips,
        earlier: trip.previous_page_cursor,
        later: trip.next_page_cursor,
    })
}

#[derive(Deserialize)]
struct Features {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
    geometry: Geometry,
}

#[derive(Deserialize)]
struct Properties {
    id: Option<String>,
    name: Option<String>,
    layer: Option<String>,
    #[serde(default)]
    category: Vec<String>,
    locality: Option<String>,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(default)]
    coordinates: Vec<f64>,
}

#[derive(Deserialize)]
struct Answer {
    data: Option<Data>,
    errors: Option<Vec<GraphError>>,
}

#[derive(Deserialize)]
struct GraphError {
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    stop_place: Option<Board>,
    quay: Option<Board>,
    trip: Option<WireTrip>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Board {
    #[serde(default)]
    estimated_calls: Vec<Call>,
}

/// A vehicle at a platform.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Call {
    aimed_arrival_time: Option<WireTime>,
    expected_arrival_time: Option<WireTime>,
    aimed_departure_time: Option<WireTime>,
    expected_departure_time: Option<WireTime>,
    #[serde(default)]
    realtime: bool,
    #[serde(default)]
    cancellation: bool,
    destination_display: Option<DestinationDisplay>,
    /// The platform the vehicle uses now.
    quay: Option<Quay>,
    scheduled_quay: Option<Quay>,
    service_journey: Option<ServiceJourney>,
}

impl Call {
    fn direction(&self) -> Option<String> {
        self.destination_display
            .as_ref()
            .and_then(|display| display.front_text.clone())
    }

    /// The platforms according to the timetable and as known now.
    fn platforms(&self) -> (Option<String>, Option<String>) {
        let code = |quay: &Option<Quay>| quay.as_ref().and_then(|quay| quay.public_code.clone());
        let now = code(&self.quay);
        (
            code(&self.scheduled_quay).or_else(|| now.clone()),
            now.filter(|_| self.realtime),
        )
    }

    fn into_stopover(self, network: &Network) -> Result<Stopover, Error> {
        let (scheduled_platform, realtime_platform) = self.platforms();
        let time = |time: Option<WireTime>| time.map(|time| time.0);
        // Without real-time data the server repeats the planned times.
        let live = |time: Option<WireTime>| time.filter(|_| self.realtime).map(|time| time.0);
        Ok(Stopover {
            scheduled_arrival: time(self.aimed_arrival_time),
            realtime_arrival: live(self.expected_arrival_time),
            scheduled_departure: time(self.aimed_departure_time),
            realtime_departure: live(self.expected_departure_time),
            scheduled_platform,
            realtime_platform,
            cancelled: self.cancellation,
            stop: self
                .quay
                .map(|quay| quay.into_location(network))
                .ok_or_else(|| Error::Parse {
                    message: "a call has no platform".to_owned(),
                })?,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DestinationDisplay {
    front_text: Option<String>,
}

/// A platform.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Quay {
    id: Option<String>,
    name: Option<String>,
    public_code: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    time_zone: Option<String>,
    stop_place: Option<StopPlace>,
}

impl Quay {
    fn into_location(self, network: &Network) -> Location {
        Location {
            id: self.id.unwrap_or_default(),
            global_ids: Vec::new(),
            kind: LocationKind::Stop,
            name: self
                .stop_place
                .and_then(|stop| stop.name)
                .or(self.name)
                .unwrap_or_default(),
            locality: None,
            coordinate: self
                .latitude
                .zip(self.longitude)
                .map(|(latitude, longitude)| Coordinate {
                    latitude,
                    longitude,
                }),
            timezone: self.time_zone.or_else(|| network.timezone.clone()),
            network: None,
        }
    }
}

#[derive(Deserialize)]
struct StopPlace {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ServiceJourney {
    line: Option<WireLine>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLine {
    public_code: Option<String>,
    name: Option<String>,
    transport_mode: Option<String>,
    transport_submode: Option<String>,
    authority: Option<Authority>,
}

impl WireLine {
    fn into_line(self) -> Line {
        let submode = self
            .transport_submode
            .filter(|submode| submode != "unknown");
        Line {
            name: self.public_code.or(self.name).unwrap_or_default(),
            mode: match (self.transport_mode.as_deref(), submode.as_deref()) {
                (Some("rail"), Some("highSpeedRail")) => Mode::HighSpeedTrain,
                (Some("rail"), Some("longDistance" | "international" | "interregionalRail")) => {
                    Mode::LongDistanceTrain
                }
                (Some("rail"), Some("nightRail")) => Mode::NightTrain,
                (Some("rail"), Some("suburbanRailway")) => Mode::SuburbanTrain,
                (Some("rail"), _) => Mode::RegionalTrain,
                (Some("air"), _) => Mode::Airplane,
                (Some("bus" | "trolleybus"), _) => Mode::Bus,
                (Some("coach"), _) => Mode::Coach,
                (Some("metro"), _) => Mode::Subway,
                (Some("tram"), _) => Mode::Tram,
                (Some("water"), _) => Mode::Ferry,
                (Some("cableway" | "lift"), _) => Mode::CableCar,
                (Some("funicular"), _) => Mode::Funicular,
                (Some("taxi"), _) => Mode::OnDemand,
                _ => Mode::Other,
            },
            product: submode,
            operator: self.authority.and_then(|authority| authority.name),
        }
    }
}

#[derive(Deserialize)]
struct Authority {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireTrip {
    previous_page_cursor: Option<String>,
    next_page_cursor: Option<String>,
    #[serde(default)]
    trip_patterns: Vec<Pattern>,
    #[serde(default)]
    routing_errors: Vec<RoutingError>,
}

#[derive(Deserialize)]
struct RoutingError {
    code: Option<String>,
}

#[derive(Deserialize)]
struct Pattern {
    #[serde(default)]
    legs: Vec<WireLeg>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLeg {
    mode: Option<String>,
    aimed_start_time: WireTime,
    expected_start_time: Option<WireTime>,
    aimed_end_time: WireTime,
    expected_end_time: Option<WireTime>,
    #[serde(default)]
    realtime: bool,
    from_place: WirePlace,
    to_place: WirePlace,
    line: Option<WireLine>,
    from_estimated_call: Option<Call>,
    to_estimated_call: Option<Call>,
    #[serde(default)]
    intermediate_estimated_calls: Vec<Call>,
}

impl WireLeg {
    fn into_leg(self, network: &Network) -> Result<Leg, Error> {
        let direction = self.from_estimated_call.as_ref().and_then(Call::direction);
        let cancelled = |call: &Option<Call>| call.as_ref().is_some_and(|call| call.cancellation);
        let end =
            |place: WirePlace, call: &Option<Call>, planned: WireTime, now: Option<WireTime>| {
                let (scheduled_platform, realtime_platform) =
                    call.as_ref().map_or_default(Call::platforms);
                LegEnd {
                    stop: place.into_location(network),
                    scheduled_time: planned.0,
                    // Without real-time data the server repeats the planned times.
                    realtime_time: now.filter(|_| self.realtime).map(|time| time.0),
                    scheduled_platform,
                    realtime_platform,
                    cancelled: cancelled(call),
                }
            };
        Ok(Leg {
            cancelled: cancelled(&self.from_estimated_call),
            departure: end(
                self.from_place,
                &self.from_estimated_call,
                self.aimed_start_time,
                self.expected_start_time,
            ),
            arrival: end(
                self.to_place,
                &self.to_estimated_call,
                self.aimed_end_time,
                self.expected_end_time,
            ),
            kind: match (self.line, self.mode.as_deref()) {
                (Some(line), _) => LegKind::Transit {
                    line: line.into_line(),
                    direction,
                },
                (None, Some("foot")) => LegKind::Walk,
                _ => LegKind::Other,
            },
            intermediate_stops: self
                .intermediate_estimated_calls
                .into_iter()
                .map(|call| call.into_stopover(network))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// An end of a leg: a platform, or any other place by its position.
#[derive(Deserialize)]
struct WirePlace {
    name: Option<String>,
    latitude: f64,
    longitude: f64,
    quay: Option<Quay>,
}

impl WirePlace {
    fn into_location(self, network: &Network) -> Location {
        if let Some(quay) = self.quay {
            return quay.into_location(network);
        }
        Location {
            // The server has no ID for a place that is no platform.
            id: String::new(),
            global_ids: Vec::new(),
            kind: LocationKind::Other,
            name: self.name.unwrap_or_default(),
            locality: None,
            coordinate: Some(Coordinate {
                latitude: self.latitude,
                longitude: self.longitude,
            }),
            timezone: network.timezone.clone(),
            network: None,
        }
    }
}
