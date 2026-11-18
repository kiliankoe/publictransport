use crate::{
    Attribution, Coverage, Departure, DeparturesQuery, Error, HafasOptions, Header, HttpRequest,
    HttpResponse, IdScheme, Location, LocationKind, LocationQuery, Operation, Place, Trips,
    TripsQuery, efa, hafas, motis, otp, trias,
};

/// One server that answers public transport queries.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug))]
pub struct Network {
    /// A short identifier, e.g. `vvo-efa`. It has to be unique among the
    /// networks an application uses, because a [`Location`] names its
    /// network by it.
    pub id: String,
    /// The name to show to users.
    pub name: String,
    /// The URL of the server. For EFA and MOTIS it is the base URL that the
    /// protocol's paths are appended to, e.g.
    /// `https://efa.vvo-online.de/std3` or `https://api.transitous.org/api`,
    /// and a trailing slash is allowed. TRIAS and OJP have one URL for all
    /// requests, e.g. `https://efa.vvo-online.de/std3/trias`, and so have
    /// HAFAS mgate, e.g. `https://fahrinfo.vbb.de/bin/mgate.exe`, and
    /// `OpenTripPlanner`, e.g.
    /// `https://api.entur.io/journey-planner/v3/graphql`.
    pub endpoint: String,
    /// The protocol the server speaks.
    pub protocol: Protocol,
    /// The IANA name of the time zone the server works in, e.g.
    /// `Europe/Berlin`. EFA needs it for requests that carry a time and
    /// HAFAS for nearly all. Locations get it as their time zone where the
    /// server sends none per stop, as MOTIS and `OpenTripPlanner` do.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub timezone: Option<String>,
    /// Identifies the client to a TRIAS or OJP server, which refuses
    /// requests without it. Public servers document a value such as
    /// `OpenService`; others hand out a key.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub requestor_ref: Option<String>,
    /// Where the network has data.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub coverage: Coverage,
    /// Whom to name as the source of the data.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub attribution: Option<Attribution>,
    /// The operations the server is known not to answer. Building a request
    /// for one of them fails with [`Error::Unsupported`].
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub unsupported: Vec<Operation>,
    /// Whether the server only answers with a key that its operator hands
    /// out on request. The application sets it as
    /// [`Network::requestor_ref`].
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub requires_key: bool,
    /// A link to usage terms that restrict who may use the server, for
    /// example to non-commercial projects. `None` if anyone may. The library
    /// does not enforce them: read them before using the network.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub terms: Option<String>,
    /// HTTP headers that every request to the server carries, for servers
    /// that want a key or the name of the client in one. A request to
    /// [`Network::geocoder`] carries them too, so do not name a geocoder of
    /// someone else next to a key for the server. A header here
    /// takes the place of one with the same name that the protocol or the
    /// HTTP client would set, such as `User-Agent`. The crate does not
    /// check names and values; the HTTP client rejects invalid ones. Debug
    /// output leaves the values out.
    #[cfg_attr(feature = "uniffi", uniffi(default = []))]
    pub headers: Vec<Header>,
    /// The URL of a geocoder in the Pelias format, such as
    /// `https://api.entur.io/geocoder/v1`, for protocols without a search
    /// for locations of their own. Location searches go to `autocomplete`
    /// below it, so it must not have a query string; without it they fail
    /// with [`Error::Unsupported`]. The kinds of a search follow Entur's
    /// use of the format: stops are in the layer `venue`, everything else
    /// in `address`.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub geocoder: Option<String>,
    /// What a HAFAS mgate server needs. Requests to such a network fail
    /// without it.
    #[cfg_attr(feature = "uniffi", uniffi(default = None))]
    pub hafas: Option<HafasOptions>,
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
    /// HAFAS mgate, the JSON interface behind the apps of operators that
    /// run the HAFAS journey planner. It needs [`Network::hafas`]. The
    /// operators made it for their own apps and have not opened it to
    /// others, so a server can change or lock others out without notice.
    HafasMgate,
    /// `OpenTripPlanner` 2 with its Transmodel API, a GraphQL interface
    /// used in the Nordic countries. It has no search for locations, so it
    /// needs [`Network::geocoder`] for that.
    OtpTransmodel,
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
            coverage: Coverage::default(),
            attribution: None,
            unsupported: Vec::new(),
            requires_key: false,
            terms: None,
            headers: Vec::new(),
            hafas: None,
            geocoder: None,
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

    /// Sets [`Network::geocoder`].
    #[must_use]
    pub fn with_geocoder(mut self, url: impl Into<String>) -> Self {
        self.geocoder = Some(url.into());
        self
    }

    /// Sets [`Network::hafas`].
    #[must_use]
    pub fn with_hafas(mut self, options: HafasOptions) -> Self {
        self.hafas = Some(options);
        self
    }

    /// Adds one of the [`Network::headers`].
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(Header {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Adds the [`Network::headers`] to a request the protocol built. One
    /// of them takes the place of a header of the protocol with its name.
    fn sent(&self, mut request: HttpRequest) -> HttpRequest {
        let replaced = |header: &Header| {
            let same = |other: &Header| other.name.eq_ignore_ascii_case(&header.name);
            self.headers.iter().any(same)
        };
        request.headers.retain(|header| !replaced(header));
        request.headers.extend(self.headers.iter().cloned());
        request
    }

    /// Records this network as the origin of a parsed location.
    fn mark(&self, location: &mut Location) {
        location.network = Some(self.id.clone());
    }

    /// Whether this network returned the location.
    fn returned(&self, location: &Location) -> bool {
        location.network.as_deref() == Some(self.id.as_str())
    }

    /// The scheme of global IDs that the protocol takes where it takes a
    /// stop ID. The German EFA, TRIAS and OJP servers take an IFOPT ID; that
    /// every server of these protocols does is an assumption. MOTIS IDs
    /// carry the name of a feed, and HAFAS and `OpenTripPlanner` servers
    /// take their own IDs only.
    fn global_ids(&self) -> Option<IdScheme> {
        match self.protocol {
            Protocol::Efa | Protocol::Trias | Protocol::Ojp1 => Some(IdScheme::Ifopt),
            Protocol::Motis | Protocol::HafasMgate | Protocol::OtpTransmodel => None,
        }
    }

    fn check(&self, operation: Operation) -> Result<(), Error> {
        if self.unsupported.contains(&operation) {
            let requests = match operation {
                Operation::Locations => "location searches",
                Operation::Departures => "departure requests",
                Operation::Trips => "trip requests",
            };
            return Err(Error::Unsupported {
                message: format!("{} does not answer {requests}", self.id),
            });
        }
        Ok(())
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl Network {
    /// The ID that names the location as a stop on this network. A
    /// [`DeparturesQuery`] with a location as its stop is sent with it.
    ///
    /// For a stop this network returned, that is its ID. For a stop from
    /// another network, or one without a network, it is the global ID of
    /// its station where the protocol takes one. A platform then becomes
    /// its whole station, and the server may still not know the stop.
    ///
    /// `None` if the location is not a stop, or if this network could only
    /// find it with a search by name, as MOTIS for a stop from another
    /// network.
    #[must_use]
    pub fn stop_id(&self, location: &Location) -> Option<String> {
        if location.kind != LocationKind::Stop {
            return None;
        }
        if self.returned(location) {
            return Some(location.id.clone());
        }
        let scheme = self.global_ids()?;
        location
            .station_ids()
            .into_iter()
            .find(|id| id.scheme == scheme)
            .map(|id| id.value)
    }

    /// The location as an end of a trip on this network: the stop as for
    /// [`Network::stop_id`], otherwise the coordinate. A [`TripsQuery`]
    /// with a location as an end is sent with it. A trip planned from a
    /// coordinate starts with a walk to a stop nearby.
    ///
    /// `None` if the location has neither and is not from this network.
    #[must_use]
    pub fn place(&self, location: &Location) -> Option<Place> {
        self.stop_id(location)
            .map(Place::Id)
            .or_else(|| location.coordinate.map(Place::Coordinate))
            .or_else(|| {
                (self.returned(location) && !location.id.is_empty())
                    .then(|| Place::Id(location.id.clone()))
            })
    }

    /// Builds the request that searches for locations by name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidNetwork`] if a TRIAS or OJP network has no
    /// requestor reference or a HAFAS network lacks [`Network::hafas`] or
    /// has options that are no JSON objects, and
    /// [`Error::InvalidQuery`] if the query asks
    /// only for kinds of location the protocol does not have or holds text
    /// the protocol cannot carry. An `OpenTripPlanner` network without a
    /// [`Network::geocoder`] gives [`Error::Unsupported`].
    pub fn locations_request(&self, query: &LocationQuery) -> Result<HttpRequest, Error> {
        self.check(Operation::Locations)?;
        let request = match self.protocol {
            Protocol::Efa => Ok(efa::locations_request(self, query)),
            Protocol::Motis => motis::locations_request(self, query),
            Protocol::Trias | Protocol::Ojp1 => trias::locations_request(self, query),
            Protocol::HafasMgate => hafas::locations_request(self, query),
            Protocol::OtpTransmodel => otp::locations_request(self, query),
        }?;
        Ok(self.sent(request))
    }

    /// Parses the response to a [`Network::locations_request`]. The best
    /// match comes first. A search without matches gives an empty list.
    ///
    /// # Errors
    ///
    /// Returns an error if the status code reports a failure or if the body
    /// is not what the protocol defines.
    pub fn parse_locations(&self, response: &HttpResponse) -> Result<Vec<Location>, Error> {
        let mut locations = match self.protocol {
            Protocol::Efa => efa::parse_locations(self, response),
            Protocol::Motis => motis::parse_locations(response),
            Protocol::Trias | Protocol::Ojp1 => trias::parse_locations(self, response),
            Protocol::HafasMgate => hafas::parse_locations(self, response),
            Protocol::OtpTransmodel => otp::parse_locations(self, response),
        }?;
        for location in &mut locations {
            self.mark(location);
        }
        Ok(locations)
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
    /// if a TRIAS or OJP network has no requestor reference or a HAFAS
    /// network no usable [`Network::hafas`] or no time zone, which the
    /// answer needs, and [`Error::InvalidQuery`] if the time is outside the
    /// supported range, if the stop is a position, or if it is a location
    /// that the network cannot name as a stop, see [`Network::stop_id`].
    pub fn departures_request(&self, query: &DeparturesQuery) -> Result<HttpRequest, Error> {
        self.check(Operation::Departures)?;
        let request = match self.protocol {
            Protocol::Efa => efa::departures_request(self, query),
            Protocol::Motis => motis::departures_request(self, query),
            Protocol::Trias | Protocol::Ojp1 => trias::departures_request(self, query),
            Protocol::HafasMgate => hafas::departures_request(self, query),
            Protocol::OtpTransmodel => otp::departures_request(self, query),
        }?;
        Ok(self.sent(request))
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
    /// an [`Error::Server`]. A HAFAS answer needs the network's time zone
    /// and is an [`Error::InvalidNetwork`] without it.
    pub fn parse_departures(&self, response: &HttpResponse) -> Result<Vec<Departure>, Error> {
        let mut departures = match self.protocol {
            Protocol::Efa => efa::parse_departures(self, response),
            Protocol::Motis => motis::parse_departures(response),
            Protocol::Trias | Protocol::Ojp1 => trias::parse_departures(self, response),
            Protocol::HafasMgate => hafas::parse_departures(self, response),
            Protocol::OtpTransmodel => otp::parse_departures(self, response),
        }?;
        for departure in &mut departures {
            self.mark(&mut departure.stop);
        }
        Ok(departures)
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
    /// if a TRIAS or OJP network has no requestor reference or a HAFAS
    /// network no usable [`Network::hafas`] or no time zone, which the
    /// answer needs, and [`Error::InvalidQuery`] if the time is outside the
    /// supported range, if an end is a coordinate off the globe or a
    /// location that the network cannot name, see [`Network::place`], or
    /// if the page is not a reference from this network. MOTIS, HAFAS and
    /// `OpenTripPlanner` send the page on unchecked, and their servers
    /// reject a foreign one.
    pub fn trips_request(&self, query: &TripsQuery) -> Result<HttpRequest, Error> {
        self.check(Operation::Trips)?;
        let request = match self.protocol {
            Protocol::Efa => efa::trips_request(self, query),
            Protocol::Motis => motis::trips_request(self, query),
            Protocol::Trias | Protocol::Ojp1 => trias::trips_request(self, query),
            Protocol::HafasMgate => hafas::trips_request(self, query),
            Protocol::OtpTransmodel => otp::trips_request(self, query),
        }?;
        Ok(self.sent(request))
    }

    /// Parses the response to a [`Network::trips_request`]. The trips are in
    /// the order the server ranks them. No connection between the two places
    /// gives an empty list.
    ///
    /// [`Trips::earlier`] and [`Trips::later`] continue the search. MOTIS,
    /// HAFAS and `OpenTripPlanner` send such references. `OpenTripPlanner`
    /// searches a window of time, so an answer can have no trips and still
    /// a reference to later ones, as at night. For EFA, TRIAS and OJP the
    /// crate makes them
    /// from the times of the trips: later trips leave at least a minute
    /// after the last of these, earlier ones arrive at least a minute
    /// before the first. A trip within that minute is missed. On TRIAS and
    /// OJP a search without [`TripsQuery::limit`] repeats one trip.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] if the network does not know one of the
    /// two places, and another error if the status code reports a failure or
    /// if the body is not what the protocol defines. A HAFAS answer needs
    /// the network's time zone and is an [`Error::InvalidNetwork`] without
    /// it.
    ///
    /// The trips are as the server planned them. Some may have left already
    /// or may no longer work out because a vehicle is late; that is for the
    /// application to check against its clock.
    pub fn parse_trips(&self, response: &HttpResponse) -> Result<Trips, Error> {
        let mut trips = match self.protocol {
            Protocol::Efa => efa::parse_trips(self, response),
            Protocol::Motis => motis::parse_trips(response),
            Protocol::Trias | Protocol::Ojp1 => trias::parse_trips(self, response),
            Protocol::HafasMgate => hafas::parse_trips(self, response),
            Protocol::OtpTransmodel => otp::parse_trips(self, response),
        }?;
        for leg in trips.trips.iter_mut().flat_map(|trip| &mut trip.legs) {
            self.mark(&mut leg.departure.stop);
            self.mark(&mut leg.arrival.stop);
            for stopover in &mut leg.intermediate_stops {
                self.mark(&mut stopover.stop);
            }
        }
        Ok(trips)
    }
}
