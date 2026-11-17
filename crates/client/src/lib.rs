//! HTTP client for public transport APIs: EFA, MOTIS, TRIAS and OJP.
//!
//! The [`publictransport`] crate builds requests and parses responses
//! without doing I/O. This crate sends them with [`reqwest`]. It re-exports
//! `publictransport` and `reqwest`, so an application depends on this crate
//! alone. The futures need a Tokio runtime, as all of `reqwest` does.
//!
//! ```no_run
//! use publictransport_client::{
//!     Client, DeparturesQuery, LocationKind, LocationQuery, Network, Protocol,
//! };
//!
//! # async fn example() -> Result<(), publictransport_client::Error> {
//! let client = Client::new("my-app/1.0 (+https://example.org)")?;
//! let vvo = Network::new("vvo", "VVO", "https://efa.vvo-online.de/std3", Protocol::Efa)
//!     .with_timezone("Europe/Berlin");
//!
//! let query = LocationQuery::new("Hauptbahnhof").with_kinds([LocationKind::Stop]);
//! let stops = client.locations(&vvo, &query).await?;
//! if let Some(stop) = stops.first() {
//!     for departure in client.departures(&vvo, &DeparturesQuery::new(&stop.id)).await? {
//!         println!("{} to {:?}", departure.line.name, departure.direction);
//!     }
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

    /// Searches for locations by name. See
    /// [`Network::parse_locations`] for the result.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request got no answer and
    /// [`Error::Api`] for everything the server or the query got wrong.
    pub async fn locations(
        &self,
        network: &Network,
        query: &LocationQuery,
    ) -> Result<Vec<Location>, Error> {
        let response = self.send(network.locations_request(query)?).await?;
        Ok(network.parse_locations(&response)?)
    }

    /// Lists the next departures at a stop. See
    /// [`Network::parse_departures`] for the result.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request got no answer and
    /// [`Error::Api`] for everything the server or the query got wrong.
    pub async fn departures(
        &self,
        network: &Network,
        query: &DeparturesQuery,
    ) -> Result<Vec<Departure>, Error> {
        let response = self.send(network.departures_request(query)?).await?;
        Ok(network.parse_departures(&response)?)
    }

    /// Plans trips between two places. See [`Network::parse_trips`] for the
    /// result.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request got no answer and
    /// [`Error::Api`] for everything the server or the query got wrong.
    pub async fn trips(&self, network: &Network, query: &TripsQuery) -> Result<Vec<Trip>, Error> {
        let response = self.send(network.trips_request(query)?).await?;
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
