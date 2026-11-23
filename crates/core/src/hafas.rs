//! HAFAS mgate, the JSON interface behind the apps of many operators: one
//! `POST` per request, with the method and its arguments in the body.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    Coordinate, Departure, DeparturesQuery, Error, GlobalId, Header, HttpRequest, HttpResponse,
    Leg, LegEnd, LegKind, Line, Location, LocationKind, LocationQuery, Method, Mode, NearbyQuery,
    Network, Place, Remark, Stopover, Timestamp, Trip, TripTime, Trips, TripsQuery,
    http::lenient_list,
    model::{Board, Target, plain_text},
};

/// What a HAFAS mgate server needs to answer, as the operator's own app
/// sends it. The values come from that app; see
/// [`Protocol::HafasMgate`](crate::Protocol::HafasMgate).
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug, Display))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct HafasOptions {
    /// The description of the client, a JSON object as text. Its fields
    /// differ from operator to operator, so the crate passes it on as it is.
    pub client: String,
    /// The credentials, a JSON object as text, passed on like the client.
    /// Debug output leaves it out.
    pub auth: String,
    /// The name of the operator's extension of the interface.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub ext: Option<String>,
    /// The version of the interface to ask for, such as `1.33`.
    pub version: String,
    /// The language for names and remarks, such as `de`.
    #[cfg_attr(feature = "uniffi", uniffi(default = "en"))]
    pub language: String,
    /// The operator's kinds of vehicles, which give a line its mode.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(default, deserialize_with = "crate::wasm::or_default")
    )]
    pub products: Vec<HafasProduct>,
}

impl HafasOptions {
    /// Options without an extension, in English, and without products, so
    /// that every line has the mode [`Mode::Other`].
    pub fn new(
        client: impl Into<String>,
        auth: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            client: client.into(),
            auth: auth.into(),
            ext: None,
            version: version.into(),
            language: "en".to_owned(),
            products: Vec::new(),
        }
    }
}

impl HafasOptions {
    /// Sets [`HafasOptions::ext`].
    #[must_use]
    pub fn with_ext(mut self, ext: impl Into<String>) -> Self {
        self.ext = Some(ext.into());
        self
    }

    /// Sets [`HafasOptions::language`].
    #[must_use]
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Sets [`HafasOptions::products`].
    #[must_use]
    pub fn with_products(mut self, products: impl IntoIterator<Item = HafasProduct>) -> Self {
        self.products = products.into_iter().collect();
        self
    }
}

impl std::fmt::Debug for HafasOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HafasOptions")
            .field("client", &self.client)
            .field("ext", &self.ext)
            .field("version", &self.version)
            .field("language", &self.language)
            .field("products", &self.products)
            .finish_non_exhaustive()
    }
}

/// The same as the debug output, so that no way of printing the options
/// in another language shows the credentials.
impl std::fmt::Display for HafasOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

/// One kind of vehicle of a HAFAS operator.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct HafasProduct {
    /// A name for programs, such as `suburban`. It decides the
    /// [`Mode`] of the lines; an ID the crate does not know gives
    /// [`Mode::Other`].
    pub id: String,
    /// The name for people, such as `S-Bahn`.
    pub name: String,
    /// The bits that mark a line as this product in the server's answers.
    pub bitmasks: Vec<u32>,
}

/// The mode that lines of a product run in. The IDs are those of the
/// entries in transport-apis, bundled or not. They have no common
/// vocabulary: `express-train` is the high-speed train of most operators
/// and the slower express of a few, so some lines get a mode that is close
/// but not exact.
fn mode(product: &str) -> Mode {
    let id = product.to_lowercase();
    // Operators with several products of a kind number them.
    let id = id.trim_end_matches(|last: char| last.is_ascii_digit());
    match id.strip_suffix('-').unwrap_or(id) {
        "ice" | "high-speed-train" | "highspeed" | "express-train" | "national-express"
        | "nationalexpress" | "intercityexpress" | "tgv" => Mode::HighSpeedTrain,
        "express"
        | "ic-ec"
        | "ec-ic"
        | "long-distance-train"
        | "national"
        | "national-train"
        | "intercity"
        | "intercity-train"
        | "ir"
        | "ir-d"
        | "d-zug"
        | "interregional"
        | "fernverkehr"
        | "icl" => Mode::LongDistanceTrain,
        "night-train" => Mode::NightTrain,
        "regional"
        | "regional-train"
        | "regional-express"
        | "regional-express-train"
        | "regionalexpress"
        | "regio-express"
        | "re-rb"
        | "re/rb"
        | "local-train"
        | "local"
        | "region"
        | "regionalverkehr"
        | "akn"
        | "rail"
        | "oe"
        | "trains"
        | "bahn-s-bahn" => Mode::RegionalTrain,
        "suburban" | "suburban-train" | "s-bahn" | "sbahn" | "s-tog" | "rapid-transit" | "dart"
        | "commuter" | "urban-train" => Mode::SuburbanTrain,
        "subway" | "u-bahn" | "ubahn" | "metro" | "bart" => Mode::Subway,
        "tram" | "tramway" | "strassenbahn" | "stadtbahn" | "luas" | "regiotram" | "tram-metro" => {
            Mode::Tram
        }
        "bus" | "city-bus" | "stadtbus" | "regional-bus" | "regionalbus" | "express-bus"
        | "other-bus" | "added-bus" | "bus-taxi" | "rapid" => Mode::Bus,
        "fernbus" | "long-distance-bus" => Mode::Coach,
        "ferry" | "faehre" | "boat" | "schiff" | "watercraft" => Mode::Ferry,
        "cable-car" | "gondola" | "aerial-lift" | "seilbahn-zahnradbahn" => Mode::CableCar,
        "funicular" => Mode::Funicular,
        "on-call" | "oncall" | "on-demand" | "dial-a-ride" | "anruf-sammel-taxi"
        | "anrufsammeltaxi" | "ast" | "taxi" | "taxibus" => Mode::OnDemand,
        "air" => Mode::Airplane,
        _ => Mode::Other,
    }
}

fn request(network: &Network, method: &str, arguments: &Value) -> Result<HttpRequest, Error> {
    let invalid = |message: &str| Error::InvalidNetwork {
        message: message.to_owned(),
    };
    let options = network
        .hafas
        .as_ref()
        .ok_or_else(|| invalid("a HAFAS network needs its options"))?;
    let object_from = |text: &str, name: &str| {
        serde_json::from_str::<Value>(text)
            .ok()
            .filter(Value::is_object)
            .ok_or_else(|| invalid(&format!("the HAFAS {name} is not a JSON object")))
    };
    let body = object([
        ("lang", Some(json!(options.language))),
        ("client", Some(object_from(&options.client, "client")?)),
        ("ext", options.ext.as_ref().map(|ext| json!(ext))),
        ("ver", Some(json!(options.version))),
        ("auth", Some(object_from(&options.auth, "auth")?)),
        ("svcReqL", Some(json!([{"meth": method, "req": arguments}]))),
    ]);
    Ok(HttpRequest {
        method: Method::Post,
        url: network.endpoint.clone(),
        headers: vec![Header {
            name: "Content-Type".to_owned(),
            value: "application/json".to_owned(),
        }],
        body: Some(body.to_string().into_bytes()),
    })
}

/// A JSON object of the fields that have a value.
fn object<const N: usize>(fields: [(&str, Option<Value>); N]) -> Value {
    fields
        .into_iter()
        .filter_map(|(name, value)| Some((name.to_owned(), value?)))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

/// A stop, address or point of interest by its ID. A number is the short
/// ID of a stop. Anything else is the full identifier of the server, which
/// addresses and points of interest have, and stops on servers that do not
/// number them.
fn place_by_id(id: &str) -> Value {
    if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()) {
        json!({"lid": format!("A=1@L={id}@")})
    } else {
        json!({"lid": id})
    }
}

fn place(network: &Network, place: &Place) -> Result<Value, Error> {
    Ok(match place.target(network)? {
        Target::Id(id) => place_by_id(&id),
        Target::Coordinate(coordinate) => {
            // On the globe, so the millionths fit.
            #[expect(clippy::cast_possible_truncation)]
            let millionths = |degrees: f64| (degrees * 1e6).round() as i64;
            json!({"type": "C", "crd": {
                "x": millionths(coordinate.longitude),
                "y": millionths(coordinate.latitude),
            }})
        }
    })
}

/// The date and the time of day as the server wants them: wall-clock time
/// in its own zone.
fn local_time(network: &Network, time: Timestamp) -> Result<(String, String), Error> {
    let local = network.local(time)?;
    Ok((
        local.strftime("%Y%m%d").to_string(),
        local.strftime("%H%M%S").to_string(),
    ))
}

pub(crate) fn locations_request(
    network: &Network,
    query: &LocationQuery,
) -> Result<HttpRequest, Error> {
    let letters: String = [
        (LocationKind::Stop, 'S'),
        (LocationKind::Address, 'A'),
        (LocationKind::Poi, 'P'),
    ]
    .into_iter()
    .filter(|(kind, _)| query.kinds.contains(kind))
    .map(|(_, letter)| letter)
    .collect();
    if letters.is_empty() && !query.kinds.is_empty() {
        return Err(Error::InvalidQuery {
            message: "HAFAS cannot search for the asked kinds of location alone".to_owned(),
        });
    }
    let kinds = if letters.is_empty() { "ALL" } else { &letters };
    let input = object([
        // The question mark asks for names that are close to the text too.
        (
            "loc",
            Some(json!({"type": kinds, "name": format!("{}?", query.text)})),
        ),
        ("maxLoc", query.limit.map(|limit| json!(limit))),
        ("field", Some(json!("S"))),
    ]);
    request(network, "LocMatch", &json!({"input": input}))
}

/// The stops within a circle, which the server orders by distance.
pub(crate) fn nearby_request(network: &Network, query: &NearbyQuery) -> Result<HttpRequest, Error> {
    let position = query.position.checked()?;
    // The server counts in millionths of a degree.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a position on the globe fits"
    )]
    let millionths = |degrees: f64| (degrees * 1_000_000.0).round() as i64;
    let arguments = object([
        (
            "ring",
            Some(json!({
                "cCrd": {"x": millionths(position.longitude), "y": millionths(position.latitude)},
                "maxDist": query.radius(),
            })),
        ),
        ("getStops", Some(json!(true))),
        ("getPOIs", Some(json!(false))),
        ("maxLoc", query.limit.map(|limit| json!(limit))),
    ]);
    request(network, "LocGeoPos", &arguments)
}

pub(crate) fn parse_nearby(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Location>, Error> {
    let Some(found) = result::<Matches>(response, &[])? else {
        return Ok(Vec::new());
    };
    Ok(found
        .loc_l
        .into_iter()
        .map(|location| location.into_location(network))
        .collect())
}

pub(crate) fn departures_request(
    network: &Network,
    query: &DeparturesQuery,
    board: Board,
) -> Result<HttpRequest, Error> {
    // The answer holds local times, so it could not be read without one.
    network.zone()?;
    let time = query
        .time
        .map(|time| local_time(network, time))
        .transpose()?;
    let (date, time) = time.unzip();
    let arguments = object([
        (
            "type",
            Some(json!(match board {
                Board::Departures => "DEP",
                Board::Arrivals => "ARR",
            })),
        ),
        ("stbLoc", Some(place_by_id(&query.stop.stop_id(network)?))),
        ("maxJny", query.limit.map(|limit| json!(limit))),
        ("date", date.map(Value::String)),
        ("time", time.map(Value::String)),
    ]);
    request(network, "StationBoard", &arguments)
}

pub(crate) fn trips_request(network: &Network, query: &TripsQuery) -> Result<HttpRequest, Error> {
    // The answer holds local times, so it could not be read without one.
    network.zone()?;
    // A page holds the time of the search it continues.
    let time = match query.time.filter(|_| query.page.is_none()) {
        Some(TripTime::DepartAt(time) | TripTime::ArriveBy(time)) => {
            Some(local_time(network, time)?)
        }
        None => None,
    };
    let (date, time) = time.unzip();
    let arguments = object([
        ("depLocL", Some(json!([place(network, &query.origin)?]))),
        (
            "arrLocL",
            Some(json!([place(network, &query.destination)?])),
        ),
        (
            "outFrwd",
            Some(json!(!matches!(query.time, Some(TripTime::ArriveBy(_))))),
        ),
        ("outDate", date.map(Value::String)),
        ("outTime", time.map(Value::String)),
        ("numF", query.limit.map(|limit| json!(limit))),
        ("getPasslist", Some(json!(true))),
        ("ctxScr", query.page.as_ref().map(|page| json!(page))),
    ]);
    request(network, "TripSearch", &arguments)
}

/// The result of the one method a request names. `None` if the server
/// reports one of the `empty` codes, which stand for "nothing found".
fn result<T: for<'a> Deserialize<'a>>(
    response: &HttpResponse,
    empty: &[&str],
) -> Result<Option<T>, Error> {
    let failed = |code: &str, text: Option<&str>| Error::Server {
        status: 200,
        message: text.map_or_else(|| code.to_owned(), |text| format!("{code}: {text}")),
    };
    let envelope: Envelope = response.json()?;
    match envelope.err.as_deref() {
        None | Some("OK") => {}
        Some("AUTH") => return Err(Error::Unauthorized),
        Some(code) => return Err(failed(code, envelope.err_txt.as_deref())),
    }
    let missing = || Error::Parse {
        message: "the response has no result".to_owned(),
    };
    let service = envelope
        .svc_res_l
        .and_then(|services| services.into_iter().next())
        .ok_or_else(missing)?;
    match service.err.as_deref() {
        // Only now, because the result of a failed call has any shape.
        None | Some("OK") => serde_json::from_value(service.res.ok_or_else(missing)?)
            .map(Some)
            .map_err(|error| Error::Parse {
                message: error.to_string(),
            }),
        // An unknown place, and a place without stops to start from.
        Some(code @ ("LOCATION" | "H9220")) => Err(Error::NotFound {
            message: service.err_txt.unwrap_or_else(|| code.to_owned()),
        }),
        Some(code) if empty.contains(&code) => Ok(None),
        Some(code) => Err(failed(code, service.err_txt.as_deref())),
    }
}

pub(crate) fn parse_locations(
    network: &Network,
    response: &HttpResponse,
) -> Result<Vec<Location>, Error> {
    let Some(result) = result::<LocationsResult>(response, &["NO_MATCH"])? else {
        return Ok(Vec::new());
    };
    Ok(result
        .matches
        .loc_l
        .into_iter()
        .map(|location| location.into_location(network))
        .collect())
}

pub(crate) fn parse_departures(
    network: &Network,
    response: &HttpResponse,
    board: Board,
) -> Result<Vec<Departure>, Error> {
    let Some(found) = result::<StationBoard>(response, &[])? else {
        return Ok(Vec::new());
    };
    let reader = Reader::new(network, &found.common);
    let mut departures = found
        .jny_l
        .into_iter()
        .map(|journey| {
            let stop = journey.stb_stop.as_ref().ok_or_else(|| Error::Parse {
                message: "an entry of the board has no stop".to_owned(),
            })?;
            let end = reader.leg_end(&journey.date, stop, board)?;
            Ok(Departure {
                stop: end.stop,
                line: reader.line(journey.prod_x)?,
                // The origin, on a board of arrivals.
                direction: journey.dir_txt,
                scheduled_time: end.scheduled_time,
                realtime_time: end.realtime_time,
                scheduled_platform: end.scheduled_platform,
                realtime_platform: end.realtime_platform,
                cancelled: end.cancelled || journey.is_cncl,
                remarks: reader.remarks([&journey.msg_l, &stop.msg_l]),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    // The server orders them by scheduled time.
    departures.sort_by_key(Departure::expected_time);
    Ok(departures)
}

pub(crate) fn parse_trips(network: &Network, response: &HttpResponse) -> Result<Trips, Error> {
    // H890 is a search that found no trip.
    let Some(found) = result::<Connections>(response, &["H890"])? else {
        return Ok(Trips {
            trips: Vec::new(),
            earlier: None,
            later: None,
        });
    };
    let reader = Reader::new(network, &found.common);
    let trips = found
        .out_con_l
        .iter()
        .map(|connection| {
            let legs = connection
                .sec_l
                .iter()
                .map(|section| reader.leg(&connection.date, section))
                .collect::<Result<_, _>>()?;
            Ok(Trip::new(legs))
        })
        .filter_map(Result::transpose)
        .collect::<Result<_, Error>>()?;
    Ok(Trips {
        trips,
        earlier: found.out_ctx_scr_b,
        later: found.out_ctx_scr_f,
    })
}

/// The name of a platform. Older versions of the interface send the name,
/// newer ones an object with it, and a server may send both.
fn platform(name: Option<&String>, object: Option<&Platform>) -> Option<String> {
    let named = object.and_then(|platform| platform.txt.as_ref());
    name.or(named).filter(|name| !name.is_empty()).cloned()
}

/// Reads the parts of an answer that point into its shared lists.
struct Reader<'a> {
    network: &'a Network,
    common: &'a Common,
    /// An error if the network has none; only an answer with times needs it.
    zone: Result<jiff::tz::TimeZone, Error>,
}

impl<'a> Reader<'a> {
    fn new(network: &'a Network, common: &'a Common) -> Self {
        Self {
            network,
            common,
            zone: network.zone(),
        }
    }

    fn location(&self, index: Option<usize>) -> Result<Location, Error> {
        index
            .and_then(|index| self.common.locations.get(index))
            .map(|location| location.clone().into_location(self.network))
            .ok_or_else(|| Error::Parse {
                message: "a stop refers to a location the response does not list".to_owned(),
            })
    }

    fn line(&self, index: Option<usize>) -> Result<Line, Error> {
        let product = index
            .and_then(|index| self.common.lines.get(index))
            .ok_or_else(|| Error::Parse {
                message: "a journey refers to a line the response does not list".to_owned(),
            })?;
        let products = self.network.hafas.as_ref().map(|options| &options.products);
        let kind = products.into_iter().flatten().find(|kind| {
            kind.bitmasks
                .iter()
                .any(|bits| bits & product.cls.unwrap_or(0) != 0)
        });
        Ok(Line {
            name: product.name.clone().unwrap_or_default(),
            mode: kind.map_or(Mode::Other, |kind| mode(&kind.id)),
            product: product
                .prod_ctx
                .as_ref()
                .and_then(|context| context.cat_out_l.as_deref())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            operator: product
                .opr_x
                .and_then(|index| self.common.operators.get(index))
                .and_then(|operator| operator.name.clone()),
        })
    }

    /// A time of day on or after `date`: `HHMMSS`, with the number of days
    /// to add in front if the time is on a later day.
    ///
    /// Newer versions of the interface send the offset from UTC in minutes
    /// with a time. It tells apart the two times a clock shows in the hour
    /// that is repeated when daylight saving time ends; without it the
    /// first is taken. Everything else follows the time zone, because a
    /// real-time time may come with the offset of the scheduled one.
    fn time(&self, date: &str, time: &str, offset: Option<i32>) -> Result<Timestamp, Error> {
        let unreadable = || Error::Parse {
            message: format!("{date} {time} is not a date and a time"),
        };
        let (days, time) = time
            .len()
            .checked_sub(6)
            .and_then(|at| time.split_at_checked(at))
            .ok_or_else(unreadable)?;
        let days: i64 = if days.is_empty() {
            0
        } else {
            days.parse().map_err(|_| unreadable())?
        };
        let local = jiff::civil::DateTime::strptime("%Y%m%d%H%M%S", format!("{date}{time}"))
            .ok()
            .filter(|_| date.len() == 8)
            .ok_or_else(unreadable)?;
        let local = jiff::Span::new()
            .try_days(days)
            .and_then(|days| local.checked_add(days))
            .map_err(|_| unreadable())?;
        let ambiguous = self.zone.clone()?.to_ambiguous_timestamp(local);
        let time = match ambiguous.offset() {
            jiff::tz::AmbiguousOffset::Fold { after, .. }
                if offset.is_some_and(|minutes| {
                    i64::from(minutes) * 60 == i64::from(after.seconds())
                }) =>
            {
                ambiguous.later()
            }
            _ => ambiguous.compatible(),
        }
        .map_err(|_| unreadable())?;
        // The check on the range of years is the one for times in a query.
        let time = Timestamp {
            unix_seconds: time.as_second(),
        };
        time.to_jiff().map_err(|_| unreadable())?;
        Ok(time)
    }

    fn optional_time(
        &self,
        date: &str,
        time: Option<&String>,
        offset: Option<i32>,
    ) -> Result<Option<Timestamp>, Error> {
        time.map(|time| self.time(date, time, offset)).transpose()
    }

    fn leg_end(&self, date: &str, stop: &Stop, side: Board) -> Result<LegEnd, Error> {
        let mut stopover = self.stopover(date, stop)?;
        // A stop can have a platform and a cancellation for each side.
        if side == Board::Arrivals {
            let arriving = |platform: Option<String>, known: &mut Option<String>| {
                if platform.is_some() {
                    *known = platform;
                }
            };
            arriving(
                platform(stop.a_platf_s.as_ref(), stop.a_pltf_s.as_ref()),
                &mut stopover.scheduled_platform,
            );
            arriving(
                platform(stop.a_platf_r.as_ref(), stop.a_pltf_r.as_ref()),
                &mut stopover.realtime_platform,
            );
        }
        stopover.cancelled = match side {
            Board::Departures => stop.d_cncl,
            Board::Arrivals => stop.a_cncl,
        };
        stopover.on(side).ok_or_else(|| Error::Parse {
            message: "a stop has no scheduled time".to_owned(),
        })
    }

    fn stopover(&self, date: &str, stop: &Stop) -> Result<Stopover, Error> {
        let (arrival, departure) = (stop.a_tz_offset, stop.d_tz_offset);
        Ok(Stopover {
            stop: self.location(stop.loc_x)?,
            scheduled_arrival: self.optional_time(date, stop.a_time_s.as_ref(), arrival)?,
            realtime_arrival: self.optional_time(
                date,
                stop.a_time_r.as_ref(),
                stop.a_tz_offset_r.or(arrival),
            )?,
            scheduled_departure: self.optional_time(date, stop.d_time_s.as_ref(), departure)?,
            realtime_departure: self.optional_time(
                date,
                stop.d_time_r.as_ref(),
                stop.d_tz_offset_r.or(departure),
            )?,
            scheduled_platform: platform(stop.d_platf_s.as_ref(), stop.d_pltf_s.as_ref())
                .or_else(|| platform(stop.a_platf_s.as_ref(), stop.a_pltf_s.as_ref())),
            realtime_platform: platform(stop.d_platf_r.as_ref(), stop.d_pltf_r.as_ref())
                .or_else(|| platform(stop.a_platf_r.as_ref(), stop.a_pltf_r.as_ref())),
            cancelled: stop.d_cncl || stop.a_cncl,
        })
    }

    /// The remarks that the given lists of a journey and of its stops refer
    /// to, each once. The operator and the occupancy come as texts too and
    /// are no remarks.
    fn remarks<'b>(&self, lists: impl IntoIterator<Item = &'b Vec<Message>>) -> Vec<Remark> {
        let mut remarks = Vec::new();
        for message in lists.into_iter().flatten() {
            if message.kind.as_deref() != Some("REM") {
                continue;
            }
            let Some(wire) = message
                .rem_x
                .and_then(|index| self.common.remarks.get(index))
                .and_then(|remark| WireRemark::deserialize(remark).ok())
            else {
                continue;
            };
            let code = wire.code.as_deref().unwrap_or_default();
            if code == "OPERATOR" || code.starts_with("text.occup") {
                continue;
            }
            let Some(text) = wire.txt_n.as_deref().and_then(plain_text) else {
                continue;
            };
            let remark = match wire.kind.as_deref() {
                Some("R") => Remark {
                    text: Some(text),
                    ..Remark::disruption()
                },
                Some("A" | "H") => Remark::hint(text, wire.code),
                _ => continue,
            };
            if !remarks.contains(&remark) {
                remarks.push(remark);
            }
        }
        remarks
    }

    fn leg(&self, date: &str, section: &Section) -> Result<Leg, Error> {
        let journey = section.jny.as_ref();
        let kind = match (section.kind.as_str(), journey) {
            ("JNY", Some(journey)) => LegKind::Transit {
                line: self.line(journey.prod_x)?,
                direction: journey.dir_txt.clone(),
            },
            // A transfer is a walk between two stops.
            ("WALK" | "TRSF", _) => LegKind::Walk,
            _ => LegKind::Other,
        };
        let calls = journey.map_or(&[][..], |journey| &journey.stop_l);
        let between = calls.get(1..calls.len().saturating_sub(1)).unwrap_or(&[]);
        Ok(Leg {
            kind,
            departure: self.leg_end(date, &section.dep, Board::Departures)?,
            arrival: self.leg_end(date, &section.arr, Board::Arrivals)?,
            intermediate_stops: between
                .iter()
                .map(|stop| self.stopover(date, stop))
                .collect::<Result<_, _>>()?,
            cancelled: journey.is_some_and(|journey| journey.is_cncl),
            remarks: self.remarks(
                journey
                    .map(|journey| &journey.msg_l)
                    .into_iter()
                    .chain([&section.dep.msg_l, &section.arr.msg_l]),
            ),
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    err: Option<String>,
    err_txt: Option<String>,
    #[serde(default)]
    svc_res_l: Option<Vec<Service>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Service {
    err: Option<String>,
    err_txt: Option<String>,
    res: Option<Value>,
}

/// The lists that the results point into by index.
#[derive(Deserialize, Default)]
struct Common {
    #[serde(default, rename = "locL")]
    locations: Vec<WireLocation>,
    #[serde(default, rename = "prodL")]
    lines: Vec<Product>,
    #[serde(default, rename = "opL")]
    operators: Vec<Operator>,
    /// Kept as they come, so that an entry of an unknown shape keeps its
    /// place in the list and costs only itself.
    #[serde(default, rename = "remL", deserialize_with = "lenient_list")]
    remarks: Vec<Value>,
}

/// A text that journeys and stops refer to.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRemark {
    /// `A` for an attribute of the ride, `H` for a hint, `R` for a message
    /// about its real-time state.
    #[serde(rename = "type")]
    kind: Option<String>,
    code: Option<String>,
    txt_n: Option<String>,
}

/// A reference from a journey or a stop to a text.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    #[serde(rename = "type")]
    kind: Option<String>,
    rem_x: Option<usize>,
}

#[derive(Deserialize)]
struct LocationsResult {
    #[serde(rename = "match")]
    matches: Matches,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Matches {
    #[serde(default)]
    loc_l: Vec<WireLocation>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct WireLocation {
    lid: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    name: Option<String>,
    ext_id: Option<String>,
    crd: Option<Position>,
    /// Global IDs as `<kind>×<id>`.
    gid_l: Option<Vec<String>>,
    /// The same in newer versions of the interface.
    global_id_l: Option<Vec<TypedId>>,
}

#[derive(Deserialize, Clone)]
struct TypedId {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

/// A position in millionths of a degree.
#[derive(Deserialize, Clone, Copy)]
struct Position {
    x: f64,
    y: f64,
}

impl WireLocation {
    fn into_location(self, network: &Network) -> Location {
        Location {
            // The short ID names a stop in a request and nothing else.
            id: self
                .ext_id
                .filter(|_| self.kind.as_deref() == Some("S"))
                .or(self.lid)
                .unwrap_or_default(),
            // The kind `A` is the one with IFOPT IDs. A server may send
            // both lists.
            global_ids: self
                .gid_l
                .iter()
                .flatten()
                .filter_map(|id| id.strip_prefix("A×"))
                .chain(
                    self.global_id_l
                        .iter()
                        .flatten()
                        .filter(|id| id.kind == "A")
                        .map(|id| id.id.as_str()),
                )
                .filter_map(GlobalId::ifopt)
                .fold(Vec::new(), |mut ids, id| {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                    ids
                }),
            kind: match self.kind.as_deref() {
                Some("S") => LocationKind::Stop,
                Some("A") => LocationKind::Address,
                Some("P") => LocationKind::Poi,
                _ => LocationKind::Other,
            },
            name: self.name.unwrap_or_default(),
            locality: None,
            coordinate: self.crd.map(|position| Coordinate {
                latitude: position.y / 1e6,
                longitude: position.x / 1e6,
            }),
            timezone: network.timezone.clone(),
            network: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    name: Option<String>,
    cls: Option<u32>,
    opr_x: Option<usize>,
    prod_ctx: Option<ProductContext>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductContext {
    cat_out_l: Option<String>,
}

#[derive(Deserialize)]
struct Operator {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationBoard {
    #[serde(default)]
    common: Common,
    #[serde(default)]
    jny_l: Vec<Journey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journey {
    #[serde(default)]
    date: String,
    prod_x: Option<usize>,
    dir_txt: Option<String>,
    #[serde(default)]
    is_cncl: bool,
    stb_stop: Option<Stop>,
    #[serde(default)]
    stop_l: Vec<Stop>,
    #[serde(default, deserialize_with = "lenient_list")]
    msg_l: Vec<Message>,
}

/// A call at a stop, with the fields of an arrival, a departure or both.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stop {
    loc_x: Option<usize>,
    a_time_s: Option<String>,
    a_time_r: Option<String>,
    d_time_s: Option<String>,
    d_time_r: Option<String>,
    #[serde(rename = "aTZOffset")]
    a_tz_offset: Option<i32>,
    #[serde(rename = "aTZOffsetR")]
    a_tz_offset_r: Option<i32>,
    #[serde(rename = "dTZOffset")]
    d_tz_offset: Option<i32>,
    #[serde(rename = "dTZOffsetR")]
    d_tz_offset_r: Option<i32>,
    a_platf_s: Option<String>,
    a_platf_r: Option<String>,
    d_platf_s: Option<String>,
    d_platf_r: Option<String>,
    a_pltf_s: Option<Platform>,
    a_pltf_r: Option<Platform>,
    d_pltf_s: Option<Platform>,
    d_pltf_r: Option<Platform>,
    #[serde(default)]
    a_cncl: bool,
    #[serde(default)]
    d_cncl: bool,
    #[serde(default, deserialize_with = "lenient_list")]
    msg_l: Vec<Message>,
}

#[derive(Deserialize)]
struct Platform {
    txt: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connections {
    #[serde(default)]
    common: Common,
    #[serde(default)]
    out_con_l: Vec<Connection>,
    out_ctx_scr_b: Option<String>,
    out_ctx_scr_f: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    date: String,
    #[serde(default)]
    sec_l: Vec<Section>,
}

#[derive(Deserialize)]
struct Section {
    #[serde(rename = "type")]
    kind: String,
    dep: Stop,
    arr: Stop,
    jny: Option<Journey>,
}
