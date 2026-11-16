//! The JSON interface (`outputFormat=rapidJSON`) of MDV's EFA journey planner.

use serde::Deserialize;

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, HttpRequest, HttpResponse, Line, Location,
    LocationKind, LocationQuery, Mode, Network, time::WireTime,
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
    let limit = query.limit.map(|limit| limit.to_string());
    let mut parameters = common();
    parameters.extend([("type_sf", "any"), ("name_sf", query.text.as_str())]);
    if let Some(limit) = &limit {
        parameters.push(("anyMaxSizeHitList", limit));
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
    // The server reads the time as wall-clock time in its own zone.
    let local_time = query
        .time
        .map(|time| {
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
        })
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
    // Unconfirmed: no recorded response contains a cancelled departure. Other
    // clients read these two fields.
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
            global_id: (wire.is_global_id == Some(true)).then(|| wire.id.clone()),
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

impl StopEvent {
    fn into_departure(self, network: &Network) -> Departure {
        let mut wire = self;
        let platforms = wire.location.properties.take();
        let transportation = wire.transportation;
        let product = transportation.product;
        let train_type = transportation
            .properties
            .as_ref()
            .and_then(|properties| non_empty(properties.train_type.clone()));
        let has_line = transportation
            .properties
            .and_then(|properties| properties.line_display)
            .is_none_or(|display| display != "TRAIN");
        let number = non_empty(transportation.number);
        Departure {
            stop: wire.location.into_location(network),
            line: Line {
                name: match (has_line, &train_type, number) {
                    (false, Some(train_type), Some(number)) => format!("{train_type} {number}"),
                    (_, _, number) => number.or(transportation.name).unwrap_or_default(),
                },
                mode: mode(
                    product.as_ref().and_then(|product| product.class),
                    has_line,
                    train_type.as_deref(),
                ),
                product: product.and_then(|product| non_empty(product.name)),
                operator: transportation
                    .operator
                    .and_then(|operator| non_empty(operator.name)),
            },
            direction: transportation
                .destination
                .and_then(|destination| non_empty(destination.name)),
            scheduled_time: wire.departure_time_planned.0,
            realtime_time: wire.departure_time_estimated.map(|time| time.0),
            scheduled_platform: platforms
                .as_ref()
                .and_then(|platforms| non_empty(platforms.planned_platform_name.clone())),
            realtime_platform: platforms.and_then(|platforms| non_empty(platforms.platform_name)),
            cancelled: wire.is_cancelled == Some(true)
                || wire
                    .realtime_status
                    .iter()
                    .any(|status| status == "TRIP_CANCELLED"),
        }
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
