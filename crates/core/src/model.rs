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
}

/// A place a trip can start or end at.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Location {
    /// Identifies the location on the network that returned it. Other
    /// networks do not understand it.
    pub id: String,
    /// An identifier shared across networks, in the IFOPT format that the
    /// German DHID uses, e.g. `de:14612:28`.
    pub global_id: Option<String>,
    /// What kind of place this is.
    pub kind: LocationKind,
    /// The name, without the locality where the server tells them apart.
    pub name: String,
    /// The town or city the location is in.
    pub locality: Option<String>,
    /// The position.
    pub coordinate: Option<Coordinate>,
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
