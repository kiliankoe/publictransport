/// A search for locations by name.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct LocationQuery {
    /// The text to search for, as a user typed it.
    pub text: String,
    /// The largest number of results wanted. Servers may return fewer.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct DeparturesQuery {
    /// The [`Location::id`] of a stop, from the same network.
    pub stop_id: String,
    /// The time from which on departures are wanted. `None` means now.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub time: Option<Timestamp>,
    /// How many departures are wanted. Servers treat this as a guide and may
    /// return a few more or fewer. `None` leaves the number to the server.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
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
pub struct TripsQuery {
    /// Where the trip starts.
    pub origin: Place,
    /// Where the trip ends.
    pub destination: Place,
    /// When to leave or arrive. `None` means leaving now.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub time: Option<TripTime>,
    /// How many trips are wanted. Servers treat this as a guide and may
    /// return more or fewer. `None` leaves the number to the server.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub limit: Option<u32>,
}

impl TripsQuery {
    /// Trips from `origin` to `destination` that leave now.
    #[must_use]
    pub fn new(origin: Place, destination: Place) -> Self {
        Self {
            origin,
            destination,
            time: None,
            limit: None,
        }
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

impl Place {
    /// The place as a coordinate, checked to be on the globe.
    pub(crate) fn checked(&self) -> Result<&Self, crate::Error> {
        match self {
            Self::Coordinate(coordinate)
                if !(-90.0..=90.0).contains(&coordinate.latitude)
                    || !(-180.0..=180.0).contains(&coordinate.longitude) =>
            {
                Err(crate::Error::InvalidQuery {
                    message: "the coordinate is not on the globe".to_owned(),
                })
            }
            place => Ok(place),
        }
    }
}

/// One end of a trip in a [`TripsQuery`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub enum Place {
    /// A [`Location::id`] from the same network.
    Id(String),
    /// A position, such as where the user is.
    Coordinate(Coordinate),
}

/// The time a [`TripsQuery`] is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub enum TripTime {
    /// Leave at this time or later.
    DepartAt(Timestamp),
    /// Arrive at this time or earlier.
    ArriveBy(Timestamp),
}

/// One way to get from the start to the destination.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
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
pub enum LegKind {
    /// A ride in a public transport vehicle.
    Transit {
        /// The line the vehicle runs on.
        line: Line,
        /// Where the vehicle is headed, as shown on it.
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
pub struct LegEnd {
    /// The stop, platform or other place.
    pub stop: Location,
    /// The time according to the timetable. For a walk it is the time the
    /// trip was planned with.
    pub scheduled_time: Timestamp,
    /// The predicted time. `None` if the server has no prediction, which is
    /// always the case for a walk. A walk after a late ride therefore keeps
    /// its planned times and can start before the ride arrives.
    pub realtime_time: Option<Timestamp>,
    /// The platform according to the timetable.
    pub scheduled_platform: Option<String>,
    /// The platform as far as the server knows now.
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
pub struct Stopover {
    /// The stop, platform or other place.
    pub stop: Location,
    /// The arrival time according to the timetable.
    pub scheduled_arrival: Option<Timestamp>,
    /// The predicted arrival time. `None` if the server has no prediction.
    pub realtime_arrival: Option<Timestamp>,
    /// The departure time according to the timetable.
    pub scheduled_departure: Option<Timestamp>,
    /// The predicted departure time. `None` if the server has no prediction.
    pub realtime_departure: Option<Timestamp>,
    /// The platform according to the timetable.
    pub scheduled_platform: Option<String>,
    /// The platform as far as the server knows now.
    pub realtime_platform: Option<String>,
    /// Whether the vehicle skips this stop although the timetable has it.
    pub cancelled: bool,
}
