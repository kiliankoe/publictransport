use crate::{
    Departure, DeparturesQuery, Error, HttpRequest, HttpResponse, Location, LocationQuery, Trip,
    TripsQuery, efa, motis, trias,
};

/// One server that answers public transport queries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct Network {
    /// A short identifier, e.g. `vvo`.
    pub id: String,
    /// The name to show to users.
    pub name: String,
    /// The URL of the server. For EFA and MOTIS it is the base URL that the
    /// protocol's paths are appended to, e.g.
    /// `https://efa.vvo-online.de/std3` or `https://api.transitous.org/api`,
    /// and a trailing slash is allowed. TRIAS and OJP have one URL for all
    /// requests, e.g. `https://efa.vvo-online.de/std3/trias`.
    pub endpoint: String,
    /// The protocol the server speaks.
    pub protocol: Protocol,
    /// The IANA name of the time zone the server works in, e.g.
    /// `Europe/Berlin`. EFA needs it for requests that carry a time. On every
    /// protocol but MOTIS, which sends a time zone per stop, the locations
    /// get it as their time zone.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub timezone: Option<String>,
    /// Identifies the client to a TRIAS or OJP server, which refuses
    /// requests without it. Public servers document a value such as
    /// `OpenService`; others hand out a key.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub requestor_ref: Option<String>,
}

/// The protocol a [`Network`] speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub enum Protocol {
    /// The JSON interface of MDV's EFA journey planner.
    Efa,
    /// The MOTIS API, as served by Transitous.
    Motis,
    /// TRIAS 1.2 (VDV 431-2), an XML interface.
    Trias,
    /// OJP 1.0 (CEN/TS 17118:2017), the European successor of TRIAS.
    Ojp1,
}

impl Network {
    /// Describes a server. See the fields for the meaning of the arguments.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        endpoint: impl Into<String>,
        protocol: Protocol,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            endpoint: endpoint.into(),
            protocol,
            timezone: None,
            requestor_ref: None,
        }
    }

    /// Sets [`Network::timezone`].
    #[must_use]
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = Some(timezone.into());
        self
    }

    /// Sets [`Network::requestor_ref`].
    #[must_use]
    pub fn with_requestor_ref(mut self, requestor_ref: impl Into<String>) -> Self {
        self.requestor_ref = Some(requestor_ref.into());
        self
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Network {
    /// Builds the request that searches for locations by name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidNetwork`] if a TRIAS or OJP network has no
    /// requestor reference, and [`Error::InvalidQuery`] if the query holds
    /// text the protocol cannot carry.
    pub fn locations_request(&self, query: &LocationQuery) -> Result<HttpRequest, Error> {
        match self.protocol {
            Protocol::Efa => Ok(efa::locations_request(self, query)),
            Protocol::Motis => Ok(motis::locations_request(self, query)),
            Protocol::Trias | Protocol::Ojp1 => trias::locations_request(self, query),
        }
    }

    /// Parses the response to a [`Network::locations_request`]. The best
    /// match comes first. A search without matches gives an empty list.
    ///
    /// # Errors
    ///
    /// Returns an error if the status code reports a failure or if the body
    /// is not what the protocol defines.
    pub fn parse_locations(&self, response: &HttpResponse) -> Result<Vec<Location>, Error> {
        match self.protocol {
            Protocol::Efa => efa::parse_locations(self, response),
            Protocol::Motis => motis::parse_locations(response),
            Protocol::Trias | Protocol::Ojp1 => trias::parse_locations(self, response),
        }
    }

    /// Builds the request for the next departures at a stop.
    ///
    /// EFA takes the time as local time to the minute. In the hour that
    /// repeats when daylight saving time ends, it cannot tell the first pass
    /// from the second.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidNetwork`] if the query has a time and the
    /// network needs a time zone for it but has none or an unknown one, or
    /// if a TRIAS or OJP network has no requestor reference, and
    /// [`Error::InvalidQuery`] if the time is outside the supported range.
    pub fn departures_request(&self, query: &DeparturesQuery) -> Result<HttpRequest, Error> {
        match self.protocol {
            Protocol::Efa => efa::departures_request(self, query),
            Protocol::Motis => motis::departures_request(self, query),
            Protocol::Trias | Protocol::Ojp1 => trias::departures_request(self, query),
        }
    }

    /// Parses the response to a [`Network::departures_request`]. The
    /// departures are ordered by [`Departure::expected_time`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the network says that it does not know
    /// the stop, and another error if the status code reports a failure or
    /// if the body is not what the protocol defines. TRIAS and OJP servers
    /// built on EFA answer an unknown stop with an internal error, which is
    /// an [`Error::Server`].
    pub fn parse_departures(&self, response: &HttpResponse) -> Result<Vec<Departure>, Error> {
        match self.protocol {
            Protocol::Efa => efa::parse_departures(self, response),
            Protocol::Motis => motis::parse_departures(response),
            Protocol::Trias | Protocol::Ojp1 => trias::parse_departures(self, response),
        }
    }

    /// Builds the request for trips between two places.
    ///
    /// EFA takes the time as local time to the minute, with the same limits
    /// as in [`Network::departures_request`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidNetwork`] if the query has a time and the
    /// network needs a time zone for it but has none or an unknown one, or
    /// if a TRIAS or OJP network has no requestor reference, and
    /// [`Error::InvalidQuery`] if the time is outside the supported range.
    pub fn trips_request(&self, query: &TripsQuery) -> Result<HttpRequest, Error> {
        match self.protocol {
            Protocol::Efa => efa::trips_request(self, query),
            Protocol::Motis => motis::trips_request(self, query),
            Protocol::Trias | Protocol::Ojp1 => trias::trips_request(self, query),
        }
    }

    /// Parses the response to a [`Network::trips_request`]. The trips are in
    /// the order the server ranks them. No connection between the two places
    /// gives an empty list.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the network does not know one of the
    /// two places, and another error if the status code reports a failure or
    /// if the body is not what the protocol defines.
    ///
    /// The trips are as the server planned them. Some may have left already
    /// or may no longer work out because a vehicle is late; that is for the
    /// application to check against its clock.
    pub fn parse_trips(&self, response: &HttpResponse) -> Result<Vec<Trip>, Error> {
        match self.protocol {
            Protocol::Efa => efa::parse_trips(self, response),
            Protocol::Motis => motis::parse_trips(response),
            Protocol::Trias | Protocol::Ojp1 => trias::parse_trips(self, response),
        }
    }
}
