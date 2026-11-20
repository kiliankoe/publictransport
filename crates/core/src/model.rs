/// A search for locations by name.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct LocationQuery {
    /// The text to search for, as a user typed it.
    pub text: String,
    /// The largest number of results wanted. Servers may return fewer. EFA
    /// has no limit that works for every search, so the request carries
    /// none there and the server returns every match.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub limit: Option<u32>,
    /// The kinds of location wanted. Empty asks for every kind. The request
    /// asks the server to filter, so `limit` counts only wanted locations.
    /// MOTIS, HAFAS and the geocoder of an `OpenTripPlanner` network
    /// cannot ask for the kind [`LocationKind::Other`]: it is left out
    /// there, and a query for that kind alone is an
    /// [`Error::InvalidQuery`](crate::Error::InvalidQuery). That geocoder
    /// also has addresses and points of interest in one layer, so a search
    /// for one of the two returns both.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(default, deserialize_with = "crate::wasm::or_default")
    )]
    pub kinds: Vec<LocationKind>,
}

impl<T: Into<String>> From<T> for LocationQuery {
    fn from(text: T) -> Self {
        Self::new(text)
    }
}

impl From<&Self> for LocationQuery {
    fn from(query: &Self) -> Self {
        query.clone()
    }
}

impl<T: Into<Place>> From<T> for DeparturesQuery {
    fn from(stop: T) -> Self {
        Self::new(stop)
    }
}

impl From<&Self> for DeparturesQuery {
    fn from(query: &Self) -> Self {
        query.clone()
    }
}

impl From<&Self> for TripsQuery {
    fn from(query: &Self) -> Self {
        query.clone()
    }
}

impl LocationQuery {
    /// A search for `text` without a limit, for every kind of location.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: None,
            kinds: Vec::new(),
        }
    }

    /// Sets [`LocationQuery::kinds`].
    #[must_use]
    pub fn with_kinds(mut self, kinds: impl IntoIterator<Item = LocationKind>) -> Self {
        self.kinds = kinds.into_iter().collect();
        self
    }

    /// Sets [`LocationQuery::limit`].
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// A place a trip can start or end at.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Location {
    /// Identifies the location on the network that returned it. Other
    /// networks do not understand it.
    pub id: String,
    /// The identifiers of the location that are shared across networks.
    /// Empty if the server sends none in a scheme this crate knows.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(default, deserialize_with = "crate::wasm::or_default")
    )]
    pub global_ids: Vec<GlobalId>,
    /// What kind of place this is.
    pub kind: LocationKind,
    /// The name, without the locality where the server tells them apart.
    pub name: String,
    /// The town or city the location is in.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub locality: Option<String>,
    /// The position.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub coordinate: Option<Coordinate>,
    /// The IANA name of the time zone the location is in, e.g.
    /// `Europe/Berlin`, for showing times there, as
    /// [`Location::local_time`] does. MOTIS sends it, and
    /// `OpenTripPlanner` for stops in departures and trips; otherwise it is
    /// [`Network::timezone`](crate::Network::timezone).
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub timezone: Option<String>,
    /// The [`Network::id`](crate::Network::id) of the network that returned
    /// the location. Its `id` belongs to that network;
    /// [`Network::place`](crate::Network::place) and
    /// [`Network::stop_id`](crate::Network::stop_id) translate the location
    /// for another one. A location without a network, such as one an
    /// application built itself, is treated as one from another network.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub network: Option<String>,
}

impl Location {
    /// The stations this location belongs to, one for each of
    /// [`Location::global_ids`]. A station and its platforms share them, so
    /// they tell whether two networks mean the same station.
    #[must_use]
    pub fn station_ids(&self) -> Vec<GlobalId> {
        self.global_ids.iter().map(GlobalId::station).collect()
    }

    /// Whether the two locations are the same station or parts of it, by
    /// their [`Location::station_ids`]. `false` if either has no global ID,
    /// even for two stops of one network.
    #[must_use]
    pub fn same_station(&self, other: &Self) -> bool {
        let stations = other.station_ids();
        self.station_ids().iter().any(|id| stations.contains(id))
    }
}

/// An identifier of a stop that is shared across networks.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct GlobalId {
    /// The scheme that gives the value its meaning.
    pub scheme: IdScheme,
    /// The identifier.
    pub value: String,
}

/// A scheme of identifiers for stops that several networks use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub enum IdScheme {
    /// IFOPT, the European standard for the identification of fixed
    /// objects in public transport: a country code, then parts separated by
    /// colons. The German DHID follows it: `de:14612:28` for a station,
    /// `de:14612:28:3:2` for one of its platforms.
    Ifopt,
}

impl GlobalId {
    /// `id` as an IFOPT ID, if it has the layout of one: a country code, two
    /// numbers, then up to two more parts for area and platform.
    pub(crate) fn ifopt(id: &str) -> Option<Self> {
        let parts: Vec<&str> = id.split(':').collect();
        let [country, region, stop, rest @ ..] = parts.as_slice() else {
            return None;
        };
        let is_number = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        let valid = country.len() == 2
            && country.bytes().all(|b| b.is_ascii_lowercase())
            && is_number(region)
            && is_number(stop)
            && rest.len() <= 2
            && rest.iter().all(|part| !part.is_empty());
        valid.then(|| Self {
            scheme: IdScheme::Ifopt,
            value: id.to_owned(),
        })
    }

    /// The station the stop belongs to, in the same scheme.
    fn station(&self) -> Self {
        let parts = match self.scheme {
            IdScheme::Ifopt => 3,
        };
        let end = self
            .value
            .match_indices(':')
            .nth(parts - 1)
            .map_or(self.value.len(), |(index, _)| index);
        Self {
            scheme: self.scheme,
            value: self.value.get(..end).unwrap_or(&self.value).to_owned(),
        }
    }
}

/// The kind of a [`Location`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub enum LocationKind {
    /// A stop or station.
    Stop,
    /// A street or street address.
    Address,
    /// A point of interest.
    Poi,
    /// Anything else, such as a locality.
    Other,
}

/// A WGS 84 position in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Coordinate {
    /// Latitude, positive to the north.
    pub latitude: f64,
    /// Longitude, positive to the east.
    pub longitude: f64,
}

/// An instant in UTC. [`Location::local_time`] gives it as the clocks at a
/// stop show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "unstable-wasm", derive(tsify::Tsify), tsify(type = "Date"))]
pub struct Timestamp {
    /// Seconds since 1970-01-01T00:00:00Z.
    pub unix_seconds: i64,
}

/// A time on the clocks at a place, from [`Location::local_time`]. It
/// prints as the time of day, such as `14:08`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(tsify::Tsify),
    tsify(type = "string")
)]
pub struct LocalTime {
    /// The year.
    pub year: u16,
    /// The month, 1 to 12.
    pub month: u8,
    /// The day of the month, 1 to 31.
    pub day: u8,
    /// The hour, 0 to 23.
    pub hour: u8,
    /// The minute.
    pub minute: u8,
    /// The second.
    pub second: u8,
    /// What the clocks there are ahead of UTC, in seconds. Negative to the
    /// west.
    pub utc_offset_seconds: i32,
}

impl std::fmt::Display for LocalTime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:02}:{:02}", self.hour, self.minute)
    }
}

/// A request for the next departures at a stop.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct DeparturesQuery {
    /// The stop: its [`Location::id`] on the network that is asked, or a
    /// location of any network. A position is not a stop.
    pub stop: Place,
    /// The time from which on departures are wanted. `None` means now.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub time: Option<Timestamp>,
    /// How many departures are wanted. This is not a maximum: servers treat
    /// it as a guide and may return more or fewer, so cut the list where a
    /// fixed number matters. `None` leaves the number to the server.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub limit: Option<u32>,
}

impl DeparturesQuery {
    /// The next departures at `stop` from now on. The stop is an ID, a
    /// [`Location`] or a [`Place`].
    pub fn new(stop: impl Into<Place>) -> Self {
        Self {
            stop: stop.into(),
            time: None,
            limit: None,
        }
    }

    /// Sets [`DeparturesQuery::time`].
    #[must_use]
    pub fn with_time(mut self, time: Timestamp) -> Self {
        self.time = Some(time);
        self
    }

    /// Sets [`DeparturesQuery::limit`].
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// One vehicle leaving a stop.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Departure {
    /// Where the vehicle leaves from. Servers name the platform here where
    /// they have one in their data and the station otherwise, so compare
    /// stops by [`Location::station_ids`].
    pub stop: Location,
    /// The line the vehicle runs on.
    pub line: Line,
    /// Where the vehicle is headed, as shown on it.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub direction: Option<String>,
    /// The departure time according to the timetable.
    pub scheduled_time: Timestamp,
    /// The predicted departure time, which can be earlier than scheduled.
    /// `None` if the server has no prediction; a vehicle that is on time has
    /// the scheduled time here.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_time: Option<Timestamp>,
    /// The platform according to the timetable.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub scheduled_platform: Option<String>,
    /// The platform the vehicle leaves from as far as the server knows now.
    /// It equals the scheduled one unless the platform changed.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_platform: Option<String>,
    /// Whether the departure does not take place.
    pub cancelled: bool,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Departure {
    /// The time the vehicle is expected to leave: the predicted time if
    /// there is one, otherwise the scheduled time.
    #[must_use]
    pub fn expected_time(&self) -> Timestamp {
        self.realtime_time.unwrap_or(self.scheduled_time)
    }
}

/// A line of a public transport network.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Line {
    /// The short name shown on vehicles, e.g. `7`, `S1` or `RE13`. Trains
    /// without a line have their category and number here, e.g. `ICE 1652`.
    pub name: String,
    /// The kind of vehicle.
    pub mode: Mode,
    /// The server's own name for the kind of vehicle, e.g. `Straßenbahn` or
    /// `PlusBus`. It can be finer than [`Mode`].
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub product: Option<String>,
    /// The name of the company that runs the line.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub operator: Option<String>,
}

/// The kind of vehicle that serves a [`Line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub enum Mode {
    /// A high-speed train, e.g. ICE or TGV.
    HighSpeedTrain,
    /// A long-distance train, e.g. IC or EC.
    LongDistanceTrain,
    /// A long-distance night train.
    NightTrain,
    /// A regional train.
    RegionalTrain,
    /// A suburban train, e.g. S-Bahn or RER.
    SuburbanTrain,
    /// A subway or metro.
    Subway,
    /// A tram or light rail.
    Tram,
    /// A local or regional bus.
    Bus,
    /// A long-distance bus.
    Coach,
    /// A ferry or other boat.
    Ferry,
    /// A cable car or aerial lift.
    CableCar,
    /// A funicular or rack railway.
    Funicular,
    /// A service that runs on demand, e.g. a shared taxi.
    OnDemand,
    /// A flight.
    Airplane,
    /// Anything else.
    Other,
}

/// A request for trips from one place to another.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct TripsQuery {
    /// Where the trip starts.
    pub origin: Place,
    /// Where the trip ends.
    pub destination: Place,
    /// When to leave or arrive. `None` means leaving now.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub time: Option<TripTime>,
    /// How many trips are wanted. This is not a maximum: servers treat it
    /// as a guide and may return more or fewer, so cut the list where a
    /// fixed number matters. `None` leaves the number to the server.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub limit: Option<u32>,
    /// Continues an earlier search: [`Trips::earlier`] or [`Trips::later`]
    /// of its result, set on the query that led to it. On EFA, TRIAS and
    /// OJP the page takes the place of [`TripsQuery::time`]; the other
    /// protocols continue the search the reference came from. A
    /// reference is only good for the network it came from, and its text
    /// has no meaning to rely on.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub page: Option<String>,
}

/// What the references that [`Trips::paged_by_time`] makes start with.
const LATER: &str = "departure:";
const EARLIER: &str = "arrival:";

impl TripsQuery {
    /// Trips from `origin` to `destination` that leave now. Each end is
    /// an ID, a [`Coordinate`], a [`Location`] or a [`Place`].
    #[must_use]
    pub fn new(origin: impl Into<Place>, destination: impl Into<Place>) -> Self {
        Self {
            origin: origin.into(),
            destination: destination.into(),
            time: None,
            limit: None,
            page: None,
        }
    }

    /// Sets [`TripsQuery::page`].
    #[must_use]
    pub fn with_page(mut self, reference: impl Into<String>) -> Self {
        self.page = Some(reference.into());
        self
    }

    /// The time to search at on a protocol without page references of its
    /// own: the one a reference of [`Trips::paged_by_time`] holds, otherwise
    /// the one of the query.
    pub(crate) fn paged_time(&self) -> Result<Option<TripTime>, crate::Error> {
        let Some(page) = &self.page else {
            return Ok(self.time);
        };
        let time = |text: &str| {
            text.parse()
                .ok()
                .map(|unix_seconds| Timestamp { unix_seconds })
        };
        let later = page.strip_prefix(LATER).and_then(time);
        let earlier = page.strip_prefix(EARLIER).and_then(time);
        later
            .map(TripTime::DepartAt)
            .or(earlier.map(TripTime::ArriveBy))
            .map(Some)
            .ok_or_else(|| crate::Error::InvalidQuery {
                message: "the page is not a reference from this network".to_owned(),
            })
    }

    /// Asks for trips that leave at `time` or later.
    #[must_use]
    pub fn with_departure(mut self, time: Timestamp) -> Self {
        self.time = Some(TripTime::DepartAt(time));
        self
    }

    /// Asks for trips that arrive at `time` or earlier.
    #[must_use]
    pub fn with_arrival(mut self, time: Timestamp) -> Self {
        self.time = Some(TripTime::ArriveBy(time));
        self
    }

    /// Sets [`TripsQuery::limit`].
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// A place as a request names it: what is left of a [`Place`] once the
/// network has translated a location.
pub(crate) enum Target {
    Id(String),
    Coordinate(Coordinate),
}

impl Place {
    /// The place as `network` can name it in a trip request, with a
    /// coordinate checked to be on the globe.
    pub(crate) fn target(&self, network: &crate::Network) -> Result<Target, crate::Error> {
        let invalid = |message: &str| crate::Error::InvalidQuery {
            message: message.to_owned(),
        };
        match self {
            Self::Id(id) => Ok(Target::Id(id.clone())),
            Self::Coordinate(coordinate)
                if (-90.0..=90.0).contains(&coordinate.latitude)
                    && (-180.0..=180.0).contains(&coordinate.longitude) =>
            {
                Ok(Target::Coordinate(*coordinate))
            }
            Self::Coordinate(_) => Err(invalid("the coordinate is not on the globe")),
            Self::Location(location) => network
                .place(location)
                .ok_or_else(|| {
                    invalid(&format!(
                        "{} cannot name the location {}: it has neither a stop ID that the \
                         network takes nor a coordinate",
                        network.id, location.name
                    ))
                })?
                .target(network),
        }
    }

    /// The place as the ID of a stop that `network` can ask departures
    /// for.
    pub(crate) fn stop_id(&self, network: &crate::Network) -> Result<String, crate::Error> {
        let invalid = |message: &str| crate::Error::InvalidQuery {
            message: message.to_owned(),
        };
        match self {
            Self::Id(id) => Ok(id.clone()),
            Self::Coordinate(_) => Err(invalid("departures need a stop, not a position")),
            Self::Location(location) => network.stop_id(location).ok_or_else(|| {
                invalid(&format!(
                    "{} cannot name the location {} as a stop: it is no stop, or it comes from \
                     another network and has no global ID that this one takes",
                    network.id, location.name
                ))
            }),
        }
    }
}

impl From<&str> for Place {
    fn from(id: &str) -> Self {
        Self::Id(id.to_owned())
    }
}

impl From<&String> for Place {
    fn from(id: &String) -> Self {
        Self::Id(id.clone())
    }
}

impl From<&Self> for Place {
    fn from(place: &Self) -> Self {
        place.clone()
    }
}

impl From<String> for Place {
    fn from(id: String) -> Self {
        Self::Id(id)
    }
}

impl From<Coordinate> for Place {
    fn from(coordinate: Coordinate) -> Self {
        Self::Coordinate(coordinate)
    }
}

impl From<Location> for Place {
    fn from(location: Location) -> Self {
        Self::Location(location)
    }
}

impl From<&Location> for Place {
    fn from(location: &Location) -> Self {
        Self::Location(location.clone())
    }
}

/// A place in a query: the stop of a [`DeparturesQuery`] or one end of a
/// trip in a [`TripsQuery`]. An ID, a position and a location convert into
/// it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
// Read by hand in wasm.rs, so that an object is told apart by its keys.
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, tsify::Tsify),
    serde(untagged)
)]
pub enum Place {
    /// A [`Location::id`] from the same network, or another ID the network
    /// understands.
    Id(String),
    /// A position, such as where the user is. Departures need a stop, so a
    /// position is an [`Error::InvalidQuery`](crate::Error::InvalidQuery)
    /// there.
    Coordinate(Coordinate),
    /// A location from a search, a departure or a trip, of this network or
    /// another. The network that is asked translates it with
    /// [`Network::stop_id`](crate::Network::stop_id) for departures and
    /// [`Network::place`](crate::Network::place) for trips. A location it
    /// cannot name is an
    /// [`Error::InvalidQuery`](crate::Error::InvalidQuery).
    Location(Location),
}

/// The time a [`TripsQuery`] is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub enum TripTime {
    /// Leave at this time or later.
    DepartAt(Timestamp),
    /// Arrive at this time or earlier.
    ArriveBy(Timestamp),
}

/// The answer to a [`TripsQuery`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Trips {
    /// The trips, in the order the server ranks them.
    pub trips: Vec<Trip>,
    /// A reference for the trips before these, for [`TripsQuery::page`].
    /// `None` if the server sent none or, on a protocol without references
    /// of its own, if there are no trips to continue from. The trips of an
    /// earlier page are in the same order as these, so they belong in
    /// front of them as a whole.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub earlier: Option<String>,
    /// A reference for the trips after these, with the same rules. An
    /// `OpenTripPlanner` server searches a window of an hour or two, so at
    /// night its answer can have no trips and this reference: follow it,
    /// a few times if need be, before telling a user that there are none.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub later: Option<String>,
}

impl Trips {
    /// The trips with references made from their times, for protocols that
    /// have none: later trips leave after the last of these, earlier ones
    /// arrive before the first. A minute apart, so that a server which
    /// counts in minutes does not return the same trip again.
    pub(crate) fn paged_by_time(trips: Vec<Trip>) -> Self {
        let time = |end: Option<&LegEnd>| {
            end.map(|end| end.realtime_time.unwrap_or(end.scheduled_time).unix_seconds)
        };
        let last = trips.iter().filter_map(|trip| time(trip.departure())).max();
        let first = trips.iter().filter_map(|trip| time(trip.arrival())).min();
        Self {
            later: last.map(|time| format!("{LATER}{}", time.saturating_add(60))),
            earlier: first.map(|time| format!("{EARLIER}{}", time.saturating_sub(60))),
            trips,
        }
    }
}

/// One way to get from the start to the destination.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Trip {
    /// The parts of the trip in the order they are traveled. Never empty.
    pub legs: Vec<Leg>,
}

impl Trip {
    /// Where and when the trip starts. `None` only if `legs` is empty, which
    /// it never is in a trip this crate parsed.
    #[must_use]
    pub fn departure(&self) -> Option<&LegEnd> {
        self.legs.first().map(|leg| &leg.departure)
    }

    /// Where and when the trip ends. `None` only if `legs` is empty.
    #[must_use]
    pub fn arrival(&self) -> Option<&LegEnd> {
        self.legs.last().map(|leg| &leg.arrival)
    }

    /// The number of rides minus one. Walks do not count.
    #[must_use]
    pub fn transfers(&self) -> usize {
        self.legs
            .iter()
            .filter(|leg| matches!(leg.kind, LegKind::Transit { .. }))
            .count()
            .saturating_sub(1)
    }
}

/// One part of a [`Trip`]: a ride in one vehicle, or a walk.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Leg {
    /// How this part is traveled.
    pub kind: LegKind,
    /// Where and when the leg starts.
    pub departure: LegEnd,
    /// Where and when the leg ends.
    pub arrival: LegEnd,
    /// The stops the vehicle calls at between the two ends.
    pub intermediate_stops: Vec<Stopover>,
    /// Whether the ride does not take place, or does not call at the stop
    /// where the leg starts or ends.
    pub cancelled: bool,
}

/// How a [`Leg`] is traveled.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(tag = "type", rename_all = "camelCase")
)]
pub enum LegKind {
    /// A ride in a public transport vehicle.
    Transit {
        /// The line the vehicle runs on.
        line: Line,
        /// Where the vehicle is headed, as shown on it.
        #[cfg_attr(
            feature = "unstable-wasm",
            serde(skip_serializing_if = "Option::is_none")
        )]
        direction: Option<String>,
    },
    /// A walk, including one inside a station to change vehicles.
    Walk,
    /// Any other way to travel, such as a bike or a taxi.
    Other,
}

/// The start or the end of a [`Leg`]. The fields follow the same rules as
/// those of a [`Departure`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct LegEnd {
    /// The stop, platform or other place.
    pub stop: Location,
    /// The time according to the timetable. For a walk it is the time the
    /// trip was planned with.
    pub scheduled_time: Timestamp,
    /// The predicted time. `None` if the server has no prediction, which is
    /// always the case for a walk. A walk after a late ride therefore keeps
    /// its planned times and can start before the ride arrives.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_time: Option<Timestamp>,
    /// The platform according to the timetable.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub scheduled_platform: Option<String>,
    /// The platform as far as the server knows now.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_platform: Option<String>,
    /// Whether the vehicle skips this stop although the timetable has it.
    pub cancelled: bool,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl LegEnd {
    /// The predicted time if there is one, otherwise the scheduled time.
    #[must_use]
    pub fn expected_time(&self) -> Timestamp {
        self.realtime_time.unwrap_or(self.scheduled_time)
    }
}

impl Stopover {
    /// The stopover as the start of a leg, if it has a departure time.
    pub(crate) fn into_leg_start(self) -> Option<LegEnd> {
        Some(LegEnd {
            stop: self.stop,
            scheduled_time: self.scheduled_departure?,
            realtime_time: self.realtime_departure,
            scheduled_platform: self.scheduled_platform,
            realtime_platform: self.realtime_platform,
            cancelled: self.cancelled,
        })
    }

    /// The stopover as the end of a leg, if it has an arrival time.
    pub(crate) fn into_leg_end(self) -> Option<LegEnd> {
        Some(LegEnd {
            stop: self.stop,
            scheduled_time: self.scheduled_arrival?,
            realtime_time: self.realtime_arrival,
            scheduled_platform: self.scheduled_platform,
            realtime_platform: self.realtime_platform,
            cancelled: self.cancelled,
        })
    }
}

/// A call at a stop: where it is and when the vehicle arrives and leaves.
/// The fields follow the same rules as those of a [`Departure`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
#[cfg_attr(
    feature = "unstable-wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify),
    serde(rename_all = "camelCase")
)]
pub struct Stopover {
    /// The stop, platform or other place.
    pub stop: Location,
    /// The arrival time according to the timetable.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub scheduled_arrival: Option<Timestamp>,
    /// The predicted arrival time. `None` if the server has no prediction.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_arrival: Option<Timestamp>,
    /// The departure time according to the timetable.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub scheduled_departure: Option<Timestamp>,
    /// The predicted departure time. `None` if the server has no prediction.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_departure: Option<Timestamp>,
    /// The platform according to the timetable.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub scheduled_platform: Option<String>,
    /// The platform as far as the server knows now.
    #[cfg_attr(
        feature = "unstable-wasm",
        serde(skip_serializing_if = "Option::is_none")
    )]
    pub realtime_platform: Option<String>,
    /// Whether the vehicle skips this stop although the timetable has it.
    pub cancelled: bool,
}
