//! TRIAS 1.2 and OJP 1.0, two XML interfaces with the same structure.
//!
//! OJP renamed some elements and moved the envelope into the SIRI namespace.
//! The wire types below accept both sets of names, and [`Dialect`] writes the
//! requests of either.
//!
//! Requests leave out `RequestTimestamp`, which the schemas require. The
//! element holds the current time, this crate has no clock, and the servers
//! do not read it.

use quick_xml::escape::escape;
use serde::Deserialize;

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, GlobalId, Header, HttpRequest, HttpResponse,
    Leg, LegEnd, LegKind, Line, Location, LocationKind, LocationQuery, Method, Mode, Network,
    Place, Protocol, Stopover, Timestamp, Trip, TripTime, TripsQuery, time::WireTime,
};

/// How the two protocols write a request.
#[derive(Clone, Copy)]
struct Dialect {
    ojp: bool,
}

impl Dialect {
    fn of(network: &Network) -> Self {
        Self {
            ojp: network.protocol == Protocol::Ojp1,
        }
    }

    /// An element of the protocol's own namespace.
    fn element(self, name: &str, content: &str) -> String {
        let prefix = if self.ojp { "ojp:" } else { "" };
        format!("<{prefix}{name}>{content}</{prefix}{name}>")
    }

    /// An element with text in it.
    fn text(self, name: &str, text: &str) -> String {
        self.element(name, &escaped(text))
    }

    /// A service request by its TRIAS name.
    fn request(self, name: &str, content: &str) -> String {
        if self.ojp {
            self.element(&format!("OJP{name}"), content)
        } else {
            self.element(name, content)
        }
    }

    /// A reference to a place, with the time bound to it if there is one.
    fn place(self, name: &str, place: &Place, time: Option<&str>) -> String {
        let (reference, label) = match place {
            Place::Id(id) => {
                let stop = if self.ojp {
                    "StopPlaceRef"
                } else {
                    "StopPointRef"
                };
                (self.text(stop, id), id.clone())
            }
            Place::Coordinate(coordinate) => {
                // The coordinates are SIRI elements in OJP, where SIRI is
                // the default namespace.
                let (longitude, latitude) = (coordinate.longitude, coordinate.latitude);
                let position = if self.ojp {
                    format!("<Longitude>{longitude}</Longitude><Latitude>{latitude}</Latitude>")
                } else {
                    self.element("Longitude", &longitude.to_string())
                        + &self.element("Latitude", &latitude.to_string())
                };
                (
                    self.element("GeoPosition", &position),
                    format!("{latitude},{longitude}"),
                )
            }
        };
        // The schemas demand a name next to the reference. The servers do not
        // read it.
        let label = self.element("LocationName", &self.text("Text", &label));
        let reference = self.element(
            if self.ojp { "PlaceRef" } else { "LocationRef" },
            &(reference + &label),
        );
        let time = time.map_or_else(String::new, |time| self.text("DepArrTime", time));
        self.element(name, &(reference + &time))
    }

    fn number_of_results(self, limit: Option<u32>) -> String {
        limit.map_or_else(String::new, |limit| {
            self.element("NumberOfResults", &limit.to_string())
        })
    }

    /// The HTTP request that carries a service request.
    fn post(self, network: &Network, request: &str) -> Result<HttpRequest, Error> {
        let requestor = network
            .requestor_ref
            .as_deref()
            .filter(|requestor| !requestor.is_empty() && is_xml_text(requestor))
            .ok_or_else(|| Error::InvalidNetwork {
                message: "a TRIAS or OJP network needs a requestor reference".to_owned(),
            })?;
        let requestor = escaped(requestor);
        let body = if self.ojp {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><OJP xmlns="http://www.siri.org.uk/siri" xmlns:ojp="http://www.vdv.de/ojp" version="1.0"><OJPRequest><ServiceRequest><RequestorRef>{requestor}</RequestorRef>{request}</ServiceRequest></OJPRequest></OJP>"#
            )
        } else {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><Trias xmlns="http://www.vdv.de/trias" xmlns:siri="http://www.siri.org.uk/siri" version="1.2"><ServiceRequest><siri:RequestorRef>{requestor}</siri:RequestorRef><RequestPayload>{request}</RequestPayload></ServiceRequest></Trias>"#
            )
        };
        Ok(HttpRequest {
            method: Method::Post,
            url: network.endpoint.clone(),
            headers: vec![Header {
                name: "Content-Type".to_owned(),
                value: "application/xml".to_owned(),
            }],
            body: Some(body.into_bytes()),
        })
    }
}

/// Text as the content of an element. A carriage return needs a character
/// reference, because XML parsers turn a literal one into a line feed.
fn escaped(text: &str) -> String {
    escape(text).replace('\r', "&#13;")
}

/// Whether XML 1.0 can carry the text. Most control characters are not
/// allowed, not even as character references.
fn is_xml_text(text: &str) -> bool {
    text.chars().all(|character| {
        matches!(character, '\t' | '\n' | '\r' | ' '..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
    })
}

/// The text of a query, checked to be something a request can carry.
fn query_text(text: &str) -> Result<&str, Error> {
    if is_xml_text(text) {
        Ok(text)
    } else {
        Err(Error::InvalidQuery {
            message: "the text holds a control character that XML cannot carry".to_owned(),
        })
    }
}

fn place_text(place: &Place) -> Result<&Place, Error> {
    if let Place::Id(id) = place {
        query_text(id)?;
    }
    place.checked()
}

fn utc(time: Timestamp) -> Result<String, Error> {
    time.to_jiff().map(|time| time.to_string())
}

pub(crate) fn locations_request(
    network: &Network,
    query: &LocationQuery,
) -> Result<HttpRequest, Error> {
    let dialect = Dialect::of(network);
    let text = query_text(&query.text)?;
    let input = dialect.element("InitialInput", &dialect.text("LocationName", text));
    let types: String = [
        (LocationKind::Stop, "stop"),
        (LocationKind::Address, "address"),
        (LocationKind::Poi, "poi"),
        (
            LocationKind::Other,
            if dialect.ojp {
                "topographicPlace"
            } else {
                "locality"
            },
        ),
    ]
    .into_iter()
    .filter(|(kind, _)| query.kinds.contains(kind))
    .map(|(_, name)| dialect.element("Type", name))
    .collect();
    // The schemas want the types before the number of results.
    let restrictions = types + &dialect.number_of_results(query.limit);
    let restrictions = if restrictions.is_empty() {
        restrictions
    } else {
        dialect.element("Restrictions", &restrictions)
    };
    let request = dialect.request("LocationInformationRequest", &(input + &restrictions));
    dialect.post(network, &request)
}

pub(crate) fn departures_request(
    network: &Network,
    query: &DeparturesQuery,
) -> Result<HttpRequest, Error> {
    let dialect = Dialect::of(network);
    let time = query.time.map(utc).transpose()?;
    let location = dialect.place(
        "Location",
        &Place::Id(query_text(&query.stop_id)?.to_owned()),
        time.as_deref(),
    );
    let parameters = dialect.element(
        "Params",
        &(dialect.number_of_results(query.limit)
            + &dialect.element("StopEventType", "departure")
            + &dialect.element("IncludeRealtimeData", "true")),
    );
    let request = dialect.request("StopEventRequest", &(location + &parameters));
    dialect.post(network, &request)
}

pub(crate) fn trips_request(network: &Network, query: &TripsQuery) -> Result<HttpRequest, Error> {
    let dialect = Dialect::of(network);
    let (departure, arrival) = match query.time {
        Some(TripTime::DepartAt(time)) => (Some(utc(time)?), None),
        Some(TripTime::ArriveBy(time)) => (None, Some(utc(time)?)),
        None => (None, None),
    };
    let origin = dialect.place("Origin", place_text(&query.origin)?, departure.as_deref());
    let destination = dialect.place(
        "Destination",
        place_text(&query.destination)?,
        arrival.as_deref(),
    );
    let parameters = dialect.element(
        "Params",
        &(dialect.number_of_results(query.limit)
            + &dialect.element("IncludeIntermediateStops", "true")),
    );
    let request = dialect.request("TripRequest", &(origin + &destination + &parameters));
    dialect.post(network, &request)
}

pub(crate) fn parse_locations(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Location>, Error> {
    let (answers, failure) = answers(network, response)?;
    let answer = answer(answers.locations, &failure, response.status)?;
    let mut results = answer.results(failure, response.status, &["NORESULTS"])?;
    // Servers do not promise an order.
    results.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    let ojp = Dialect::of(network).ojp;
    Ok(results
        .into_iter()
        .filter_map(|result| result.location.into_location(network, ojp))
        .collect())
}

pub(crate) fn parse_departures(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Departure>, Error> {
    let (answers, failure) = answers(network, response)?;
    let answer = answer(answers.stop_events, &failure, response.status)?;
    let mut departures = answer
        .results(failure, response.status, &["NOEVENTFOUND"])?
        .into_iter()
        .map(|result| result.stop_event.into_departure(network))
        .collect::<Result<Vec<_>, _>>()?;
    departures.sort_by_key(Departure::expected_time);
    Ok(departures)
}

pub(crate) fn parse_trips(network: &Network, response: &HttpResponse) -> Result<Vec<Trip>, Error> {
    let (answers, failure) = answers(network, response)?;
    let answer = answer(answers.trips, &failure, response.status)?;
    let results = answer.results(
        failure,
        response.status,
        &["NOTRIPFOUND", "ORIGINDESTINATIONIDENTICAL"],
    )?;
    let mut trips = Vec::new();
    for result in results {
        let legs = result
            .trip
            .legs
            .into_iter()
            .filter_map(|leg| leg.into_leg(network).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        if !legs.is_empty() {
            trips.push(Trip { legs });
        }
    }
    Ok(trips)
}

/// The answers in a response, from where the dialect of the network puts
/// them, and what the envelope around them says about a failure.
fn answers(network: &Network, response: &HttpResponse) -> Result<(Answers, Failure), Error> {
    let envelope: Envelope =
        quick_xml::de::from_reader(response.success_body()?).map_err(|error| Error::Parse {
            message: error.to_string(),
        })?;
    let delivery = if Dialect::of(network).ojp {
        envelope
            .ojp_response
            .map(|response| response.service_delivery)
    } else {
        envelope.service_delivery
    }
    .ok_or_else(wrong_answer)?;
    let failure = Failure::new(delivery.status, Vec::new(), delivery.error_condition);
    let answers = delivery.payload.unwrap_or(Answers {
        locations: delivery.locations,
        stop_events: delivery.stop_events,
        trips: delivery.trips,
    });
    Ok((answers, failure))
}

/// The answer to one kind of request. If there is none, the envelope may
/// say why, for example that it does not accept the requestor.
fn answer<T>(
    answer: Option<Answer<T>>,
    envelope: &Failure,
    status: u16,
) -> Result<Answer<T>, Error> {
    match answer {
        Some(answer) => Ok(answer),
        None => Err(envelope
            .check(status, &[])
            .err()
            .unwrap_or_else(wrong_answer)),
    }
}

fn wrong_answer() -> Error {
    Error::Parse {
        message: "the response holds no answer to this kind of request".to_owned(),
    }
}

/// TRIAS has the delivery at the top, OJP inside `OJPResponse`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Envelope {
    service_delivery: Option<Delivery>,
    #[serde(rename = "OJPResponse")]
    ojp_response: Option<OjpResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct OjpResponse {
    service_delivery: Delivery,
}

/// TRIAS wraps the answers in `DeliveryPayload`, OJP has them directly
/// under other names.
#[derive(Deserialize)]
struct Delivery {
    #[serde(rename = "Status")]
    status: Option<bool>,
    #[serde(rename = "ErrorCondition")]
    error_condition: Option<ErrorCondition>,
    #[serde(rename = "DeliveryPayload")]
    payload: Option<Answers>,
    #[serde(rename = "OJPLocationInformationDelivery")]
    locations: Option<Answer<LocationResult>>,
    #[serde(rename = "OJPStopEventDelivery")]
    stop_events: Option<Answer<StopEventResult>>,
    #[serde(rename = "OJPTripDelivery")]
    trips: Option<Answer<TripResult>>,
}

#[derive(Deserialize)]
struct Answers {
    #[serde(rename = "LocationInformationResponse")]
    locations: Option<Answer<LocationResult>>,
    #[serde(rename = "StopEventResponse")]
    stop_events: Option<Answer<StopEventResult>>,
    #[serde(rename = "TripResponse")]
    trips: Option<Answer<TripResult>>,
}

/// The results of one request, or the reason there are none. TRIAS reports
/// errors as `ErrorMessage`, OJP as `ErrorCondition` with a status.
#[derive(Deserialize)]
struct Answer<T> {
    #[serde(
        default = "Vec::new",
        rename = "LocationResult",
        alias = "Location",
        alias = "StopEventResult",
        alias = "TripResult"
    )]
    results: Vec<T>,
    #[serde(rename = "Status")]
    status: Option<bool>,
    #[serde(default, rename = "ErrorMessage")]
    error_messages: Vec<ErrorMessage>,
    #[serde(rename = "ErrorCondition")]
    error_condition: Option<ErrorCondition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ErrorMessage {
    #[serde(default)]
    text: Vec<Text>,
}

/// A SIRI error: one element that names the kind of error and may hold a
/// text, and a description.
#[derive(Deserialize)]
struct ErrorCondition {
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(default, rename = "$value")]
    kinds: Vec<ErrorKind>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ErrorKind {
    error_text: Option<String>,
}

impl<T> Answer<T> {
    /// The results, or the error that explains why there are none. Errors
    /// whose code ends in one of `no_results` only say that there is
    /// nothing to report.
    fn results(self, envelope: Failure, status: u16, no_results: &[&str]) -> Result<Vec<T>, Error> {
        if self.results.is_empty() {
            let mut failure = Failure::new(self.status, self.error_messages, self.error_condition);
            failure.failed |= envelope.failed;
            failure.texts.extend(envelope.texts);
            failure.check(status, no_results)?;
        }
        Ok(self.results)
    }
}

/// What a response says went wrong.
struct Failure {
    /// Whether the response reports an error at all, with or without text.
    failed: bool,
    /// Error codes such as `STOPEVENT_NOEVENTFOUND`, or free text.
    texts: Vec<String>,
}

impl Failure {
    fn new(
        status: Option<bool>,
        messages: Vec<ErrorMessage>,
        condition: Option<ErrorCondition>,
    ) -> Self {
        let failed = status == Some(false) || !messages.is_empty() || condition.is_some();
        let condition = condition.into_iter().flat_map(|condition| {
            condition
                .kinds
                .into_iter()
                .filter_map(|kind| kind.error_text)
                .chain(condition.description)
        });
        let mut texts: Vec<String> = messages
            .into_iter()
            .filter_map(|message| text(message.text))
            .chain(condition)
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
            .collect();
        texts.dedup();
        Self { failed, texts }
    }

    /// Turns the failure into the crate's error, unless every text has a
    /// code that ends in one of `no_results`.
    fn check(&self, status: u16, no_results: &[&str]) -> Result<(), Error> {
        // The part after the prefix of a code: `LOCATIONUNKNOWN` in
        // `STOPEVENT_LOCATIONUNKNOWN`.
        let code = |text: &str| {
            let is_code = text.chars().all(|c| c.is_ascii_uppercase() || c == '_');
            is_code.then(|| {
                text.rsplit_once('_')
                    .map_or(text, |(_, code)| code)
                    .to_owned()
            })
        };
        let not_found = |text: &&String| {
            code(text).is_some_and(|code| {
                code == "LOCATIONUNSERVED" || (code.len() > 7 && code.ends_with("UNKNOWN"))
            })
        };
        let harmless =
            |text: &&String| code(text).is_some_and(|code| no_results.contains(&code.as_str()));
        if let Some(text) = self.texts.iter().find(not_found) {
            return Err(Error::NotFound {
                message: text.clone(),
            });
        }
        match self.texts.iter().find(|text| !harmless(text)) {
            Some(text) => Err(Error::Server {
                status,
                message: text.clone(),
            }),
            None if self.failed && self.texts.is_empty() => Err(Error::Server {
                status,
                message: "the server reported an error without a text".to_owned(),
            }),
            None => Ok(()),
        }
    }
}

/// A text with its language, which is not read. The schemas allow one per
/// language.
#[derive(Deserialize)]
struct Text {
    #[serde(default, rename = "Text")]
    text: Vec<String>,
}

/// The first text that is not empty.
fn text(texts: Vec<Text>) -> Option<String> {
    texts
        .into_iter()
        .flat_map(|text| text.text)
        .map(|text| text.trim().to_owned())
        .find(|text| !text.is_empty())
}

/// A reference or code without the whitespace around it.
fn trimmed(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LocationResult {
    location: WireLocation,
    #[serde(default)]
    probability: f64,
}

/// A place of one of four kinds.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WireLocation {
    #[serde(alias = "StopPlace")]
    stop_point: Option<StopPoint>,
    point_of_interest: Option<PointOfInterest>,
    address: Option<Address>,
    #[serde(alias = "TopographicPlace")]
    locality: Option<Locality>,
    /// The locality in TRIAS, the full name in OJP.
    #[serde(default)]
    location_name: Vec<Text>,
    geo_position: Option<GeoPosition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StopPoint {
    #[serde(alias = "StopPlaceRef")]
    stop_point_ref: String,
    #[serde(default, alias = "StopPlaceName")]
    stop_point_name: Vec<Text>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PointOfInterest {
    point_of_interest_code: String,
    #[serde(default)]
    point_of_interest_name: Vec<Text>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Address {
    address_code: String,
    #[serde(default)]
    address_name: Vec<Text>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Locality {
    #[serde(alias = "TopographicPlaceCode")]
    locality_code: String,
    #[serde(default, alias = "TopographicPlaceName")]
    locality_name: Vec<Text>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GeoPosition {
    longitude: f64,
    latitude: f64,
}

impl WireLocation {
    /// `None` for a place of a kind this crate does not know.
    fn into_location(self, network: &Network, ojp: bool) -> Option<Location> {
        let (kind, id, name, global_ids) = if let Some(stop) = self.stop_point {
            let id = stop.stop_point_ref.trim().to_owned();
            (
                LocationKind::Stop,
                id.clone(),
                text(stop.stop_point_name),
                // Some servers number stops of their own.
                GlobalId::ifopt(&id).into_iter().collect(),
            )
        } else if let Some(poi) = self.point_of_interest {
            let name = text(poi.point_of_interest_name);
            (
                LocationKind::Poi,
                poi.point_of_interest_code,
                name,
                Vec::new(),
            )
        } else if let Some(address) = self.address {
            let name = text(address.address_name);
            (
                LocationKind::Address,
                address.address_code,
                name,
                Vec::new(),
            )
        } else {
            let locality = self.locality?;
            let name = text(locality.locality_name);
            (
                LocationKind::Other,
                locality.locality_code,
                name,
                Vec::new(),
            )
        };
        let location_name = text(self.location_name);
        Some(Location {
            id,
            global_ids,
            kind,
            name: name.or_else(|| location_name.clone()).unwrap_or_default(),
            locality: if ojp {
                // OJP has the full name here, with the locality in
                // parentheses: "Hauptbahnhof (Essen)".
                location_name.as_deref().and_then(trailing_parentheses)
            } else {
                location_name
            },
            coordinate: self.geo_position.map(|position| Coordinate {
                latitude: position.latitude,
                longitude: position.longitude,
            }),
            timezone: network.timezone.clone(),
            network: None,
        })
    }
}

/// The text in the last pair of parentheses of `name`, which can hold
/// further pairs: `Halle (Westf.)` in `Bahnhof (Halle (Westf.))`.
fn trailing_parentheses(name: &str) -> Option<String> {
    let inner = name.trim_end().strip_suffix(')')?;
    let mut depth = 1_u32;
    let start = inner.char_indices().rev().find_map(|(index, character)| {
        match character {
            ')' => depth += 1,
            '(' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(index)
    })?;
    let locality = inner.get(start + 1..)?.trim();
    (!locality.is_empty()).then(|| locality.to_owned())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StopEventResult {
    stop_event: StopEvent,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StopEvent {
    this_call: ThisCall,
    service: Service,
    extension: Option<Extension>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ThisCall {
    call_at_stop: Call,
}

/// A vehicle at a stop, or one end of a walk.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Call {
    stop_point_ref: Option<String>,
    /// The end of a walk can be another kind of place.
    address_ref: Option<String>,
    point_of_interest_ref: Option<String>,
    #[serde(default)]
    stop_point_name: Vec<Text>,
    /// The name of the end of a walk.
    #[serde(default)]
    location_name: Vec<Text>,
    geo_position: Option<GeoPosition>,
    #[serde(default, alias = "PlannedQuay")]
    planned_bay: Vec<Text>,
    #[serde(default, alias = "EstimatedQuay")]
    estimated_bay: Vec<Text>,
    service_arrival: Option<ServiceTime>,
    service_departure: Option<ServiceTime>,
    /// The vehicle passes the stop without calling.
    #[serde(default)]
    not_serviced_stop: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceTime {
    timetabled_time: WireTime,
    estimated_time: Option<WireTime>,
}

/// The vehicle. TRIAS puts the line into `ServiceSection`, OJP has it
/// directly.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Service {
    #[serde(default, rename = "ServiceSection")]
    sections: Vec<Section>,
    mode: Option<WireMode>,
    #[serde(default)]
    published_line_name: Vec<Text>,
    #[serde(default)]
    destination_text: Vec<Text>,
    #[serde(default)]
    cancelled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Section {
    mode: Option<WireMode>,
    #[serde(default)]
    published_line_name: Vec<Text>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WireMode {
    pt_mode: Option<String>,
    rail_submode: Option<String>,
    #[serde(default)]
    name: Vec<Text>,
    #[serde(default)]
    short_name: Vec<Text>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
/// What OJP adds next to the service.
struct Extension {
    #[serde(default)]
    published_journey_number: Vec<Text>,
    #[serde(default)]
    operator_name: Vec<Text>,
}

impl Call {
    fn into_stopover(self, network: &Network) -> Stopover {
        let (arrival, departure) = (self.service_arrival, self.service_departure);
        let scheduled_platform = text(self.planned_bay);
        let coordinate = self.geo_position.map(|position| Coordinate {
            latitude: position.latitude,
            longitude: position.longitude,
        });
        let stop = trimmed(self.stop_point_ref);
        let other = trimmed(self.address_ref).or(trimmed(self.point_of_interest_ref));
        Stopover {
            scheduled_arrival: arrival.as_ref().map(|time| time.timetabled_time.0),
            realtime_arrival: arrival
                .and_then(|time| time.estimated_time)
                .map(|time| time.0),
            scheduled_departure: departure.as_ref().map(|time| time.timetabled_time.0),
            realtime_departure: departure
                .and_then(|time| time.estimated_time)
                .map(|time| time.0),
            // The server only sends an estimated platform when it changed.
            realtime_platform: text(self.estimated_bay).or_else(|| scheduled_platform.clone()),
            scheduled_platform,
            cancelled: self.not_serviced_stop,
            stop: Location {
                global_ids: stop
                    .as_deref()
                    .and_then(GlobalId::ifopt)
                    .into_iter()
                    .collect(),
                kind: if stop.is_some() {
                    LocationKind::Stop
                } else {
                    LocationKind::Other
                },
                // A place without any reference is identified by its
                // position.
                id: stop.or(other).unwrap_or_else(|| {
                    coordinate.map_or_else(String::new, |coordinate| {
                        format!("{},{}", coordinate.latitude, coordinate.longitude)
                    })
                }),
                name: text(self.stop_point_name)
                    .or(text(self.location_name))
                    .unwrap_or_default(),
                locality: None,
                coordinate,
                timezone: network.timezone.clone(),
                network: None,
            },
        }
    }
}

impl Service {
    fn into_line(self, extension: Option<Extension>) -> (Line, Option<String>, bool) {
        let section = self.sections.into_iter().next().unwrap_or(Section {
            mode: self.mode,
            published_line_name: self.published_line_name,
        });
        let mut mode = section.mode;
        let (mut number, operator) = extension.map_or((None, None), |extension| {
            (
                text(extension.published_journey_number),
                text(extension.operator_name),
            )
        });
        let name = text(section.published_line_name)
            // VRR's OJP server writes this for trains without a line.
            .filter(|name| name != "empty")
            .or_else(|| {
                let category = mode
                    .as_mut()
                    .and_then(|mode| text(std::mem::take(&mut mode.short_name)));
                let number = number.take();
                match (category, number) {
                    (Some(category), Some(number)) => Some(format!("{category} {number}")),
                    (category, number) => category.or(number),
                }
            })
            .unwrap_or_default();
        let line = Line {
            name,
            mode: mode.as_ref().map_or(Mode::Other, |mode| {
                transit_mode(
                    mode.pt_mode.as_deref().map(str::trim),
                    mode.rail_submode.as_deref().map(str::trim),
                )
            }),
            product: mode.and_then(|mode| text(mode.name)),
            operator,
        };
        (line, text(self.destination_text), self.cancelled)
    }
}

impl StopEvent {
    fn into_departure(self, network: &Network) -> Result<Departure, Error> {
        let (line, direction, cancelled) = self.service.into_line(self.extension);
        let stopover = self.this_call.call_at_stop.into_stopover(network);
        Ok(Departure {
            stop: stopover.stop,
            line,
            direction,
            scheduled_time: stopover.scheduled_departure.ok_or_else(|| Error::Parse {
                message: "a stop event has no scheduled departure".to_owned(),
            })?,
            realtime_time: stopover.realtime_departure,
            scheduled_platform: stopover.scheduled_platform,
            realtime_platform: stopover.realtime_platform,
            // The vehicle may skip this stop only.
            cancelled: cancelled || stopover.cancelled,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TripResult {
    trip: WireTrip,
}

#[derive(Deserialize)]
struct WireTrip {
    #[serde(default, rename = "TripLeg")]
    legs: Vec<TripLeg>,
}

#[derive(Deserialize)]
struct TripLeg {
    #[serde(rename = "TimedLeg")]
    timed: Option<TimedLeg>,
    #[serde(rename = "InterchangeLeg", alias = "TransferLeg")]
    interchange: Option<IndividualLeg>,
    #[serde(rename = "ContinuousLeg")]
    continuous: Option<IndividualLeg>,
}

/// A ride.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TimedLeg {
    leg_board: Call,
    #[serde(default, rename = "LegIntermediates", alias = "LegIntermediate")]
    intermediates: Vec<Call>,
    leg_alight: Call,
    service: Service,
    extension: Option<Extension>,
}

/// A part of a trip without a public transport vehicle. The mode is in one
/// of four places, depending on the kind of leg and the dialect.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IndividualLeg {
    #[serde(alias = "TransferMode")]
    interchange_mode: Option<String>,
    continuous_mode: Option<String>,
    service: Option<IndividualService>,
    leg_start: Call,
    leg_end: Call,
    time_window_start: Option<WireTime>,
    time_window_end: Option<WireTime>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IndividualService {
    individual_mode: Option<String>,
}

impl TripLeg {
    /// `None` for a leg of a kind this crate does not know.
    fn into_leg(self, network: &Network) -> Result<Option<Leg>, Error> {
        let no_time = || Error::Parse {
            message: "a leg has no scheduled time at one of its ends".to_owned(),
        };
        if let Some(ride) = self.timed {
            let (line, direction, cancelled) = ride.service.into_line(ride.extension);
            return Ok(Some(Leg {
                kind: LegKind::Transit { line, direction },
                departure: ride
                    .leg_board
                    .into_stopover(network)
                    .into_leg_start()
                    .ok_or_else(no_time)?,
                arrival: ride
                    .leg_alight
                    .into_stopover(network)
                    .into_leg_end()
                    .ok_or_else(no_time)?,
                intermediate_stops: ride
                    .intermediates
                    .into_iter()
                    .map(|call| call.into_stopover(network))
                    .collect(),
                cancelled,
            }));
        }
        let Some(leg) = self.interchange.or(self.continuous) else {
            return Ok(None);
        };
        let mode = leg
            .interchange_mode
            .or(leg.continuous_mode)
            .or(leg.service.and_then(|service| service.individual_mode));
        // The schemas make the two times optional. A leg without them cannot
        // be placed in the trip and is left out.
        let (Some(start), Some(end)) = (leg.time_window_start, leg.time_window_end) else {
            return Ok(None);
        };
        let end_at = |call: Call, time: WireTime| LegEnd {
            stop: call.into_stopover(network).stop,
            scheduled_time: time.0,
            realtime_time: None,
            scheduled_platform: None,
            realtime_platform: None,
            cancelled: false,
        };
        Ok(Some(Leg {
            kind: if mode.as_deref() == Some("walk") {
                LegKind::Walk
            } else {
                LegKind::Other
            },
            departure: end_at(leg.leg_start, start),
            arrival: end_at(leg.leg_end, end),
            intermediate_stops: Vec::new(),
            cancelled: false,
        }))
    }
}

/// Maps the mode and, for trains, the submode. The values are those of the
/// SIRI and TPEG vocabularies that both protocols use.
fn transit_mode(mode: Option<&str>, rail_submode: Option<&str>) -> Mode {
    match (mode, rail_submode) {
        (Some("rail"), Some("highSpeedRail" | "highSpeedRailService")) => Mode::HighSpeedTrain,
        (Some("rail"), Some("longDistance" | "interregionalRail" | "international")) => {
            Mode::LongDistanceTrain
        }
        (Some("rail"), Some("nightRail" | "sleeperRailService")) => Mode::NightTrain,
        (Some("rail"), Some("suburbanRailway")) | (Some("urbanRail"), _) => Mode::SuburbanTrain,
        (Some("rail"), Some("replacementRailService")) | (Some("bus" | "trolleyBus"), _) => {
            Mode::Bus
        }
        (Some("rail"), _) => Mode::RegionalTrain,
        (Some("metro"), _) => Mode::Subway,
        (Some("tram"), _) => Mode::Tram,
        (Some("coach"), _) => Mode::Coach,
        (Some("water"), _) => Mode::Ferry,
        (Some("telecabin" | "cableway"), _) => Mode::CableCar,
        (Some("funicular"), _) => Mode::Funicular,
        (Some("air"), _) => Mode::Airplane,
        (Some("taxi"), _) => Mode::OnDemand,
        _ => Mode::Other,
    }
}
