use crate::{Error, HttpRequest, HttpResponse, Location, LocationQuery, efa, motis};

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
        }
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
            Protocol::Efa => efa::parse_locations(response),
            Protocol::Motis => motis::parse_locations(response),
        }
    }
}
