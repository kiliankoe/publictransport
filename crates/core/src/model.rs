/// A search for locations by name.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LocationQuery {
    /// The text to search for, as a user typed it.
    pub text: String,
    /// The largest number of results wanted. Servers may return fewer.
    pub limit: Option<u32>,
}

impl LocationQuery {
    /// A search for `text` without a limit.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: None,
        }
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
pub struct Location {
    /// Identifies the location on the network that returned it. Other
    /// networks do not understand it.
    pub id: String,
    /// An identifier shared across networks, in the layout of the German
    /// DHID: `de:14612:28` for a station, `de:14612:28:3:2` for one of its
    /// platforms.
    pub global_id: Option<String>,
    /// What kind of place this is.
    pub kind: LocationKind,
    /// The name, without the locality where the server tells them apart.
    pub name: String,
    /// The town or city the location is in.
    pub locality: Option<String>,
    /// The position.
    pub coordinate: Option<Coordinate>,
    /// The IANA name of the time zone the location is in, e.g.
    /// `Europe/Berlin`, for showing times there. MOTIS sends it; for EFA it
    /// is [`Network::timezone`](crate::Network::timezone).
    pub timezone: Option<String>,
}

impl Location {
    /// The station this location belongs to: the first three parts of
    /// [`Location::global_id`]. A station and its platforms share it, so it
    /// tells whether two networks mean the same station.
    #[must_use]
    pub fn station_id(&self) -> Option<&str> {
        let id = self.global_id.as_deref()?;
        let end = id
            .match_indices(':')
            .nth(2)
            .map_or(id.len(), |(index, _)| index);
        id.get(..end)
    }
}

/// The kind of a [`Location`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
pub struct Coordinate {
    /// Latitude, positive to the north.
    pub latitude: f64,
    /// Longitude, positive to the east.
    pub longitude: f64,
}

/// An instant in UTC. Showing it as a local time is up to the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    /// Seconds since 1970-01-01T00:00:00Z.
    pub unix_seconds: i64,
}

/// A request for the next departures at a stop.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeparturesQuery {
    /// The [`Location::id`] of a stop, from the same network.
    pub stop_id: String,
    /// The time from which on departures are wanted. `None` means now.
    pub time: Option<Timestamp>,
    /// How many departures are wanted. Servers treat this as a guide and may
    /// return a few more or fewer. `None` leaves the number to the server.
    pub limit: Option<u32>,
}

impl DeparturesQuery {
    /// The next departures at `stop_id` from now on.
    pub fn new(stop_id: impl Into<String>) -> Self {
        Self {
            stop_id: stop_id.into(),
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
pub struct Departure {
    /// Where the vehicle leaves from. Servers name the platform here where
    /// they have one in their data and the station otherwise, so compare
    /// stops by [`Location::station_id`].
    pub stop: Location,
    /// The line the vehicle runs on.
    pub line: Line,
    /// Where the vehicle is headed, as shown on it.
    pub direction: Option<String>,
    /// The departure time according to the timetable.
    pub scheduled_time: Timestamp,
    /// The predicted departure time, which can be earlier than scheduled.
    /// `None` if the server has no prediction; a vehicle that is on time has
    /// the scheduled time here.
    pub realtime_time: Option<Timestamp>,
    /// The platform according to the timetable.
    pub scheduled_platform: Option<String>,
    /// The platform the vehicle leaves from as far as the server knows now.
    /// It equals the scheduled one unless the platform changed.
    pub realtime_platform: Option<String>,
    /// Whether the departure does not take place.
    pub cancelled: bool,
}

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
pub struct Line {
    /// The short name shown on vehicles, e.g. `7`, `S1` or `RE13`. Trains
    /// without a line have their category and number here, e.g. `ICE 1652`.
    pub name: String,
    /// The kind of vehicle.
    pub mode: Mode,
    /// The server's own name for the kind of vehicle, e.g. `Straßenbahn` or
    /// `PlusBus`. It can be finer than [`Mode`].
    pub product: Option<String>,
    /// The name of the company that runs the line.
    pub operator: Option<String>,
}

/// The kind of vehicle that serves a [`Line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
