//! Client for public transport APIs: EFA, MOTIS, TRIAS, OJP, HAFAS and
//! `OpenTripPlanner`.
//!
//! This crate does no I/O. It builds HTTP requests and parses HTTP responses,
//! so each platform can send them with its own HTTP stack. Every operation is
//! a pair of methods on [`Network`]: one returns the [`HttpRequest`] to send,
//! the other turns the [`HttpResponse`] into model types.
//!
//! ```
//! use publictransport::{HttpResponse, LocationKind, LocationQuery, Network, Protocol};
//!
//! # fn main() -> Result<(), publictransport::Error> {
//! let network = Network::new(
//!     "vvo",
//!     "Verkehrsverbund Oberelbe",
//!     "https://efa.vvo-online.de/std3",
//!     Protocol::Efa,
//! );
//! let query = LocationQuery::new("Hauptbahnhof").with_kinds([LocationKind::Stop]);
//!
//! let request = network.locations_request(&query)?;
//! // Send `request` with an HTTP client and copy its answer into a response.
//! # let body = br#"{"locations":[{"id":"de:14612:28","name":"Dresden, Hauptbahnhof","type":"stop"}]}"#.to_vec();
//! let response = HttpResponse { status: 200, headers: Vec::new(), body };
//! for location in network.parse_locations(&response)? {
//!     println!("{} ({})", location.name, location.id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Departures and trips work the same way, with
//! [`Network::departures_request`] and [`Network::parse_departures`], and
//! with [`Network::trips_request`] and [`Network::parse_trips`]. An EFA
//! network needs [`Network::with_timezone`] for requests that carry a time,
//! a TRIAS or OJP network needs [`Network::with_requestor_ref`], and a
//! HAFAS network needs [`Network::with_hafas`] and a time zone. Entur
//! asks every client for its name in the header `ET-Client-Name`
//! (`company-application`) and limits clients without it, so add it with
//! [`Network::with_header`].
//!
//! [`BundledNetwork`] names networks that were checked against their live
//! servers, so that they need not be described by hand:
//! `Network::bundled(BundledNetwork::VvoEfa)`. [`Registry::bundled`] holds
//! them all, and [`Registry::covering`] finds the ones for a position.
//!
//! The HTTP client has two duties:
//!
//! - It sets a `User-Agent` header. The header identifies the application, so
//!   requests carry none. Some servers, such as Transitous, require one.
//! - It does not follow redirects and passes on every response as it is, with
//!   all headers, whatever the status. The parse methods turn failures into
//!   an [`Error`].

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

mod efa;
mod error;
#[cfg(feature = "uniffi")]
mod ffi;
mod hafas;
mod http;
mod model;
mod motis;
mod network;
mod otp;
mod registry;
mod time;
mod trias;

pub use error::Error;
pub use hafas::{HafasOptions, HafasProduct};
pub use http::{Header, HttpRequest, HttpResponse, Method};
pub use model::{
    Coordinate, Departure, DeparturesQuery, GlobalId, IdScheme, Leg, LegEnd, LegKind, Line,
    Location, LocationKind, LocationQuery, Mode, Place, Stopover, Timestamp, Trip, TripTime, Trips,
    TripsQuery,
};
pub use network::{Network, Protocol};
pub use registry::{
    Area, Attribution, BundledNetwork, Coverage, CoverageTier, CoveringNetwork, Operation, Polygon,
    Registry,
};
