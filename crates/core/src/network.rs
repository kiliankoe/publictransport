use crate::{
    Departure, DeparturesQuery, Error, HttpRequest, HttpResponse, Location, LocationQuery, efa,
    motis,
};

/// One server that answers public transport queries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Network {
    /// A short identifier, e.g. `vvo`.
    pub id: String,
    /// The name to show to users.
    pub name: String,
    /// The base URL that the protocol's paths are appended to, e.g.
    /// `https://efa.vvo-online.de/std3` or `https://api.transitous.org/api`.
    /// A trailing slash is allowed.
    pub endpoint: String,
    /// The protocol the server speaks.
    pub protocol: Protocol,
    /// The IANA name of the time zone the server works in, e.g.
    /// `Europe/Berlin`. EFA needs it for requests that carry a time, and the
    /// locations of an EFA network get it as their time zone.
    pub timezone: Option<String>,
}

/// The protocol a [`Network`] speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Protocol {
    /// The JSON interface of MDV's EFA journey planner.
    Efa,
    /// The MOTIS API, as served by Transitous.
    Motis,
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
        }
    }

    /// Sets [`Network::timezone`].
    #[must_use]
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = Some(timezone.into());
        self
    }

    /// Builds the request that searches for locations by name.
    ///
    /// # Errors
    ///
    /// Never fails today. It will once networks differ in what they support.
    pub fn locations_request(&self, query: &LocationQuery) -> Result<HttpRequest, Error> {
        match self.protocol {
            Protocol::Efa => Ok(efa::locations_request(self, query)),
            Protocol::Motis => Ok(motis::locations_request(self, query)),
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
    /// network needs a time zone for it but has none or an unknown one, and
    /// [`Error::InvalidQuery`] if the time is outside the supported range.
    pub fn departures_request(&self, query: &DeparturesQuery) -> Result<HttpRequest, Error> {
        match self.protocol {
            Protocol::Efa => efa::departures_request(self, query),
            Protocol::Motis => motis::departures_request(self, query),
        }
    }

    /// Parses the response to a [`Network::departures_request`]. The
    /// departures are ordered by [`Departure::expected_time`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the network does not know the stop,
    /// and another error if the status code reports a failure or if the body
    /// is not what the protocol defines.
    pub fn parse_departures(&self, response: &HttpResponse) -> Result<Vec<Departure>, Error> {
        match self.protocol {
            Protocol::Efa => efa::parse_departures(self, response),
            Protocol::Motis => motis::parse_departures(response),
        }
    }
}
