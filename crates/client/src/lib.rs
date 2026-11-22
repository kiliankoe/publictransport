//! HTTP client for public transport APIs: EFA, MOTIS, TRIAS, OJP, HAFAS and
//! `OpenTripPlanner`.
//!
//! The [`publictransport`] crate builds requests and parses responses
//! without doing I/O. This crate sends them with [`reqwest`]. It re-exports
//! `publictransport` and `reqwest`, so an application depends on this crate
//! alone. The futures need a Tokio runtime, as all of `reqwest` does.
//!
//! ```no_run
//! use publictransport_client::{
//!     Client, DeparturesQuery, LocationKind, LocationQuery, Network, Protocol, TripsQuery,
//! };
//!
//! # async fn example() -> Result<(), publictransport_client::Error> {
//! let client = Client::new("my-app/1.0 (+https://example.org)")?;
//! let vvo = Network::new("vvo", "VVO", "https://efa.vvo-online.de/std3", Protocol::Efa)
//!     .with_timezone("Europe/Berlin");
//!
//! // The text alone searches for every kind of location.
//! let everything = client.locations(&vvo, "Hauptbahnhof").await?;
//! // A query type carries the options of a request.
//! let query = LocationQuery::new("Hauptbahnhof").with_kinds([LocationKind::Stop]);
//! let stops = client.locations(&vvo, query).await?;
//! println!("{} locations, {} of them stops", everything.len(), stops.len());
//! if let [from, to, ..] = stops.as_slice() {
//!     for departure in client.departures(&vvo, from).await? {
//!         // The time on the clocks at the stop, where its time zone is known.
//!         if let Some(time) = departure.local_scheduled_time() {
//!             println!("{time} {} to {:?}", departure.line.name, departure.direction);
//!         }
//!     }
//!     let board = client.departures(&vvo, DeparturesQuery::new(from).with_limit(5)).await?;
//!     let found = client.trips(&vvo, TripsQuery::new(from, to)).await?;
//!     println!("{} departures, {} trips", board.len(), found.trips.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! The client does not retry, limit its request rate or cache. Those are
//! decisions for the application.
//!
//! # Features
//!
//! The features choose what `reqwest` is built with. `rustls`, `http2` and
//! `system-proxy` are on by default.
//!
//! - `rustls`: TLS through rustls. Its `aws-lc-rs` provider needs a C
//!   toolchain to build.
//! - `native-tls`: TLS through the platform's library. Turn the default
//!   features off to use it in place of `rustls`.
//! - `http2`: HTTP/2.
//! - `system-proxy`: the proxy that the environment configures.
//!
//! Without `rustls` and `native-tls` the client only reaches `http://`
//! endpoints.

use std::time::Duration;

pub use publictransport::Error as ApiError;
// The `Error` of this crate takes the place of the one in the glob.
pub use publictransport::*;
/// The HTTP client this crate is built on, for [`Client::with_http_client`]
/// and [`Error::Transport`].
pub use reqwest;

/// Why a call to a [`Client`] failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP exchange failed: there was no connection, it timed out, TLS
    /// failed, the answer broke off, or the URL built from
    /// [`Network::endpoint`] was not valid. [`Client::new`] returns it for a
    /// client that cannot be built.
    #[error("the HTTP request failed")]
    Transport(#[from] reqwest::Error),
    /// The answer was larger than any real one, so reading it was stopped.
    #[error("the response is larger than {} MB", MAX_BODY / (1024 * 1024))]
    TooLarge,
    /// The request could not be built from the network and the query, or the
    /// server's answer reports or is a failure.
    #[error(transparent)]
    Api(#[from] ApiError),
}

/// Sends requests to public transport servers. One client serves every
/// [`Network`] and shares its connections between them.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
}

const TIMEOUT: Duration = Duration::from_secs(30);

/// Real answers stay below 1 MB. The limit keeps a broken or hostile server
/// from filling the memory.
const MAX_BODY: usize = 16 * 1024 * 1024;

impl Client {
    /// Creates a client that sends `user_agent` with every request. It
    /// should name the application, its version and a way to contact its
    /// authors, e.g. `my-app/1.0 (+https://example.org)`. Some servers,
    /// such as Transitous, require that.
    ///
    /// Requests time out after 30 seconds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if `user_agent` is not a valid header
    /// value or the TLS backend cannot be set up.
    pub fn new(user_agent: &str) -> Result<Self, Error> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            // The core reports a redirect as a moved endpoint.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self { http })
    }

    /// Creates a client on an HTTP client the caller configured, e.g. with
    /// a proxy. It must not follow redirects, so that a moved endpoint is
    /// reported as [`ApiError::Moved`]. It should set a `User-Agent` and a
    /// timeout, since `reqwest` has none by default.
    #[must_use]
    pub fn with_http_client(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// Searches for locations by name. The query is the text to search
    /// for, or a [`LocationQuery`] for a search with options. See
    /// [`Network::parse_locations`] for the result.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request got no answer and
    /// [`Error::Api`] for everything the server or the query got wrong.
    pub async fn locations(
        &self,
        network: &Network,
        query: impl Into<LocationQuery>,
    ) -> Result<Vec<Location>, Error> {
        let request = network.locations_request(&query.into())?;
        let response = self.send(request).await?;
        Ok(network.parse_locations(&response)?)
    }

    /// Lists the next departures at a stop. The query is the stop, as a
    /// [`Location`] or as an ID of this network, or a [`DeparturesQuery`]
    /// for a request with options. A stop that another network returned
    /// works where this network can name it, see [`Network::stop_id`].
    /// See
    /// [`Network::parse_departures`] for the result.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request got no answer and
    /// [`Error::Api`] for everything the server or the query got wrong.
    pub async fn departures(
        &self,
        network: &Network,
        query: impl Into<DeparturesQuery>,
    ) -> Result<Vec<Departure>, Error> {
        let request = network.departures_request(&query.into())?;
        let response = self.send(request).await?;
        Ok(network.parse_departures(&response)?)
    }

    /// Plans trips between two places. The query is a [`TripsQuery`],
    /// whose ends are locations, coordinates or IDs. See
    /// [`Network::parse_trips`] for the result. Its references continue the search:
    ///
    /// ```no_run
    /// # use publictransport_client::{Client, Error, Network, TripsQuery};
    /// # async fn example(client: Client, network: Network, query: TripsQuery) -> Result<(), Error> {
    /// let mut found = client.trips(&network, &query).await?;
    /// if let Some(later) = found.later.clone() {
    ///     let more = client.trips(&network, &query.clone().with_page(later)).await?;
    ///     found.trips.extend(more.trips);
    ///     found.later = more.later;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request got no answer and
    /// [`Error::Api`] for everything the server or the query got wrong.
    pub async fn trips(
        &self,
        network: &Network,
        query: impl Into<TripsQuery>,
    ) -> Result<Trips, Error> {
        let request = network.trips_request(&query.into())?;
        let response = self.send(request).await?;
        Ok(network.parse_trips(&response)?)
    }

    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, Error> {
        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
        };
        let mut builder = self.http.request(method, request.url);
        for header in request.headers {
            builder = builder.header(header.name, header.value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let mut response = builder.send().await?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| Header {
                name: name.as_str().to_owned(),
                value: String::from_utf8_lossy(value.as_bytes()).into_owned(),
            })
            .collect();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(Error::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}
