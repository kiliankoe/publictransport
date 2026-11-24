//! The `publictransport` library as a WebAssembly module, for the
//! TypeScript package in `packages/typescript`.
//!
//! The types are those of `publictransport` with its `wasm` feature. Their
//! methods are functions here, because a value that crosses into JavaScript
//! is a plain object. The documentation of the functions is written for
//! TypeScript, where it ends up.

#![expect(
    clippy::missing_errors_doc,
    reason = "the documentation is for TypeScript, where the package documents what is thrown"
)]

use publictransport::{
    Arrival, BundledNetwork, Coordinate, Coverage, CoverageTier, CoveringNetwork, Departure,
    DeparturesQuery, Error, GlobalId, HttpRequest, HttpResponse, Journey, LegEnd, LocalTime,
    Location, LocationQuery, NearbyQuery, Network, Place, Timestamp, Trip, Trips, TripsQuery,
};
use serde::{Serialize, de::DeserializeOwned};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

/// What the WebAssembly module throws for an error of the library. The
/// package turns it into a `PublicTransportError`.
#[derive(Serialize, Tsify)]
pub struct Thrown {
    /// The error as a sentence in English.
    message: String,
    /// The error and its details.
    #[tsify(type = "Failure")]
    failure: Error,
}

/// The value to throw for an error of the library.
fn thrown(failure: Error) -> JsValue {
    let thrown = Thrown {
        message: failure.to_string(),
        failure,
    };
    match thrown.into_ts() {
        Ok(thrown) => thrown.into(),
        Err(error) => JsError::from(error).into(),
    }
}

/// The argument `name` as the Rust type. A value of another shape is thrown
/// as a `TypeError` that says what is wrong with it.
fn read<T>(name: &str, value: &Ts<T>) -> Result<T, JsValue>
where
    T: Tsify + DeserializeOwned,
{
    T::from_js(value.js_value()).map_err(|error| {
        // The text of a JavaScript error starts with its name.
        let error = error.to_string();
        let reason = error.strip_prefix("Error: ").unwrap_or(&error);
        js_sys::TypeError::new(&format!("the {name} is not valid: {reason}")).into()
    })
}

fn write<T: Tsify + Serialize>(value: &T) -> Result<Ts<T>, JsValue> {
    value.into_ts().map_err(|error| JsError::from(error).into())
}

fn write_all<T: Tsify + Serialize>(values: &[T]) -> Result<Vec<Ts<T>>, JsValue> {
    values.iter().map(write).collect()
}

/// Builds the request that searches for locations by name, for a caller
/// who sends it with an HTTP stack of their own.
#[wasm_bindgen(js_name = locationsRequest)]
pub fn locations_request(
    network: &Ts<Network>,
    query: &Ts<LocationQuery>,
) -> Result<Ts<HttpRequest>, JsValue> {
    let request = read("network", network)?.locations_request(&read("query", query)?);
    write(&request.map_err(thrown)?)
}

/// Reads the answer to a `locationsRequest`. The best match comes first.
#[wasm_bindgen(js_name = parseLocations)]
pub fn parse_locations(
    network: &Ts<Network>,
    response: &Ts<HttpResponse>,
) -> Result<Vec<Ts<Location>>, JsValue> {
    let locations = read("network", network)?.parse_locations(&read("response", response)?);
    write_all(&locations.map_err(thrown)?)
}

/// Builds the request for the stops near a position.
#[wasm_bindgen(js_name = nearbyRequest)]
pub fn nearby_request(
    network: &Ts<Network>,
    query: &Ts<NearbyQuery>,
) -> Result<Ts<HttpRequest>, JsValue> {
    let request = read("network", network)?.nearby_request(&read("query", query)?);
    write(&request.map_err(thrown)?)
}

/// Reads the answer to a `nearbyRequest`: the stops in the order of the
/// server, which is the nearest first.
#[wasm_bindgen(js_name = parseNearby)]
pub fn parse_nearby(
    network: &Ts<Network>,
    response: &Ts<HttpResponse>,
) -> Result<Vec<Ts<Location>>, JsValue> {
    let locations = read("network", network)?.parse_nearby(&read("response", response)?);
    write_all(&locations.map_err(thrown)?)
}

/// The distance between two positions in meters, along the surface of the
/// earth as a sphere.
#[wasm_bindgen(js_name = distanceMeters)]
pub fn distance_meters(from: &Ts<Coordinate>, to: &Ts<Coordinate>) -> Result<f64, JsValue> {
    Ok(read("position", from)?.distance_meters(read("other position", to)?))
}

/// Builds the request for the next departures at a stop.
#[wasm_bindgen(js_name = departuresRequest)]
pub fn departures_request(
    network: &Ts<Network>,
    query: &Ts<DeparturesQuery>,
) -> Result<Ts<HttpRequest>, JsValue> {
    let request = read("network", network)?.departures_request(&read("query", query)?);
    write(&request.map_err(thrown)?)
}

/// Reads the answer to a `departuresRequest`. The departures are ordered by
/// expected time.
#[wasm_bindgen(js_name = parseDepartures)]
pub fn parse_departures(
    network: &Ts<Network>,
    response: &Ts<HttpResponse>,
) -> Result<Vec<Ts<Departure>>, JsValue> {
    let departures = read("network", network)?.parse_departures(&read("response", response)?);
    write_all(&departures.map_err(thrown)?)
}

/// Builds the request for the next arrivals at a stop, from the query that
/// `departuresRequest` takes.
#[wasm_bindgen(js_name = arrivalsRequest)]
pub fn arrivals_request(
    network: &Ts<Network>,
    query: &Ts<DeparturesQuery>,
) -> Result<Ts<HttpRequest>, JsValue> {
    let request = read("network", network)?.arrivals_request(&read("query", query)?);
    write(&request.map_err(thrown)?)
}

/// Reads the answer to an `arrivalsRequest`. The arrivals are ordered by
/// their expected time.
#[wasm_bindgen(js_name = parseArrivals)]
pub fn parse_arrivals(
    network: &Ts<Network>,
    response: &Ts<HttpResponse>,
) -> Result<Vec<Ts<Arrival>>, JsValue> {
    let arrivals = read("network", network)?.parse_arrivals(&read("response", response)?);
    write_all(&arrivals.map_err(thrown)?)
}

/// Builds the request for the run of one vehicle, from the `journeyId` of
/// a departure, an arrival or a leg that the network returned.
#[wasm_bindgen(js_name = journeyRequest)]
pub fn journey_request(network: &Ts<Network>, id: &str) -> Result<Ts<HttpRequest>, JsValue> {
    let request = read("network", network)?.journey_request(id);
    write(&request.map_err(thrown)?)
}

/// Reads the answer to a `journeyRequest`: the line and every stop of the
/// run in the order of travel.
#[wasm_bindgen(js_name = parseJourney)]
pub fn parse_journey(
    network: &Ts<Network>,
    response: &Ts<HttpResponse>,
) -> Result<Ts<Journey>, JsValue> {
    let journey = read("network", network)?.parse_journey(&read("response", response)?);
    write(&journey.map_err(thrown)?)
}

/// Builds the request for trips between two places.
#[wasm_bindgen(js_name = tripsRequest)]
pub fn trips_request(
    network: &Ts<Network>,
    query: &Ts<TripsQuery>,
) -> Result<Ts<HttpRequest>, JsValue> {
    let request = read("network", network)?.trips_request(&read("query", query)?);
    write(&request.map_err(thrown)?)
}

/// Reads the answer to a `tripsRequest`. The trips are in the order the
/// server ranks them.
#[wasm_bindgen(js_name = parseTrips)]
pub fn parse_trips(
    network: &Ts<Network>,
    response: &Ts<HttpResponse>,
) -> Result<Ts<Trips>, JsValue> {
    let trips = read("network", network)?.parse_trips(&read("response", response)?);
    write(&trips.map_err(thrown)?)
}

/// Reads a network from an entry in the JSON format of the
/// [transport-apis](https://github.com/public-transport/transport-apis)
/// repository. `id` is the name of the entry's file without `.json`.
#[wasm_bindgen(js_name = networkFromJson)]
pub fn network_from_json(id: &str, json: &str) -> Result<Ts<Network>, JsValue> {
    write(&Network::from_json(id, json).map_err(thrown)?)
}

/// The network that ships with the package under this ID.
#[wasm_bindgen(js_name = bundledNetwork)]
pub fn bundled_network(id: &Ts<BundledNetwork>) -> Result<Ts<Network>, JsValue> {
    write(&Network::bundled(read("ID", id)?))
}

/// The ID that names the location as a stop on the network: its own ID for
/// a stop the network returned, the global ID of its station for a stop of
/// another network where the protocol takes one. `undefined` if the
/// location is no stop or the network cannot name it.
#[wasm_bindgen(js_name = stopId)]
pub fn stop_id(network: &Ts<Network>, location: &Ts<Location>) -> Result<Option<String>, JsValue> {
    Ok(read("network", network)?.stop_id(&read("location", location)?))
}

/// The location as an end of a trip on the network: the stop as for
/// `stopId`, otherwise the coordinate. `undefined` if the location has
/// neither and is not from this network.
#[wasm_bindgen]
pub fn place(network: &Ts<Network>, location: &Ts<Location>) -> Result<Option<Ts<Place>>, JsValue> {
    let place = read("network", network)?.place(&read("location", location)?);
    place.as_ref().map(write).transpose()
}

/// The time on the clocks at the location, in ISO 8601 format with the
/// offset from UTC that holds there at that time, such as
/// `2026-11-19T14:08:00+01:00`. `undefined` if the location has no time
/// zone or one that is not a known IANA name, so that no time is shown in a
/// zone the stop is not in.
#[wasm_bindgen(js_name = localTime)]
pub fn local_time(
    location: &Ts<Location>,
    time: &Ts<Timestamp>,
) -> Result<Option<Ts<LocalTime>>, JsValue> {
    let local = read("location", location)?.local_time(read("time", time)?);
    local.as_ref().map(write).transpose()
}

/// The stations the location belongs to, one for each of its global IDs. A
/// station and its platforms share them, so they tell whether two networks
/// mean the same station.
#[wasm_bindgen(js_name = stationIds)]
pub fn station_ids(location: &Ts<Location>) -> Result<Vec<Ts<GlobalId>>, JsValue> {
    write_all(&read("location", location)?.station_ids())
}

/// Whether the two locations are the same station or parts of it, by their
/// station IDs. `false` if either has no global ID.
#[wasm_bindgen(js_name = sameStation)]
pub fn same_station(location: &Ts<Location>, other: &Ts<Location>) -> Result<bool, JsValue> {
    Ok(read("location", location)?.same_station(&read("other location", other)?))
}

/// The time a vehicle is expected at a departure, an arrival or an end of a leg: the
/// predicted time if there is one, otherwise the scheduled time.
#[wasm_bindgen(js_name = expectedTime)]
pub fn expected_time(
    // A departure and an arrival have every field of an end of a leg, with
    // the same rules.
    #[wasm_bindgen(unchecked_param_type = "Departure | Arrival | LegEnd")] end: &Ts<LegEnd>,
) -> Result<Ts<Timestamp>, JsValue> {
    write(&read("departure, arrival or end of a leg", end)?.expected_time())
}

/// The scheduled time on the clocks at the stop of a departure, an arrival or an end
/// of a leg, as `localTime` gives it. `undefined` where that is, such as
/// for a stop without a known time zone.
#[wasm_bindgen(js_name = localScheduledTime)]
pub fn local_scheduled_time(
    #[wasm_bindgen(unchecked_param_type = "Departure | Arrival | LegEnd")] end: &Ts<LegEnd>,
) -> Result<Option<Ts<LocalTime>>, JsValue> {
    let local = read("departure, arrival or end of a leg", end)?.local_scheduled_time();
    local.as_ref().map(write).transpose()
}

/// The expected time on the clocks at the stop of a departure, an arrival or an end
/// of a leg, with the same rule for `undefined`.
#[wasm_bindgen(js_name = localExpectedTime)]
pub fn local_expected_time(
    #[wasm_bindgen(unchecked_param_type = "Departure | Arrival | LegEnd")] end: &Ts<LegEnd>,
) -> Result<Option<Ts<LocalTime>>, JsValue> {
    let local = read("departure, arrival or end of a leg", end)?.local_expected_time();
    local.as_ref().map(write).transpose()
}

/// How late the vehicle is at a departure, an arrival or an end of a leg, in
/// seconds. Negative if it is early, zero if it is on time, `undefined`
/// without real-time data.
#[wasm_bindgen(js_name = delaySeconds)]
pub fn delay_seconds(
    #[wasm_bindgen(unchecked_param_type = "Departure | Arrival | LegEnd")] end: &Ts<LegEnd>,
) -> Result<Option<f64>, JsValue> {
    Ok(read("departure, arrival or end of a leg", end)?
        .delay_seconds()
        .map(number))
}

/// The platform to show for a departure, an arrival or an end of a leg: the one the
/// server knows now if there is one, otherwise the one of the timetable.
#[wasm_bindgen]
pub fn platform(
    #[wasm_bindgen(unchecked_param_type = "Departure | Arrival | LegEnd")] end: &Ts<LegEnd>,
) -> Result<Option<String>, JsValue> {
    Ok(read("departure, arrival or end of a leg", end)?
        .platform()
        .map(str::to_owned))
}

/// How long the trip takes, in seconds: from the expected time of its
/// departure to the expected time of its arrival.
#[wasm_bindgen(js_name = durationSeconds)]
pub fn duration_seconds(trip: &Ts<Trip>) -> Result<f64, JsValue> {
    Ok(number(trip_with_legs(trip)?.duration_seconds()))
}

/// Seconds as a JavaScript number. An `i64` would be a `BigInt` there, and
/// the difference of two `Date` values fits a number exactly.
#[expect(clippy::cast_precision_loss)]
fn number(seconds: i64) -> f64 {
    seconds as f64
}

/// Where and when the trip starts.
#[wasm_bindgen]
pub fn departure(trip: &Ts<Trip>) -> Result<Ts<LegEnd>, JsValue> {
    write(trip_with_legs(trip)?.departure())
}

/// Where and when the trip ends.
#[wasm_bindgen]
pub fn arrival(trip: &Ts<Trip>) -> Result<Ts<LegEnd>, JsValue> {
    write(trip_with_legs(trip)?.arrival())
}

/// A trip from the package always has a leg. One written by hand without
/// any is refused here, where the core would panic.
fn trip_with_legs(trip: &Ts<Trip>) -> Result<Trip, JsValue> {
    let trip = read("trip", trip)?;
    if trip.legs.is_empty() {
        return Err(js_sys::TypeError::new("the trip is not valid: it has no legs").into());
    }
    Ok(trip)
}

/// How often the traveler changes vehicles: the number of rides minus one,
/// and zero for a trip without a ride. Walks are not rides.
#[wasm_bindgen]
pub fn transfers(trip: &Ts<Trip>) -> Result<u32, JsValue> {
    Ok(u32::try_from(read("trip", trip)?.transfers()).unwrap_or(u32::MAX))
}

/// The best tier of the coverage that has a polygon containing the
/// position. Region codes are not used.
#[wasm_bindgen(js_name = tierAt)]
pub fn tier_at(
    coverage: &Ts<Coverage>,
    position: &Ts<Coordinate>,
) -> Result<Option<Ts<CoverageTier>>, JsValue> {
    let tier = read("coverage", coverage)?.tier_at(read("position", position)?);
    tier.as_ref().map(write).transpose()
}

/// A set of networks to choose from.
#[wasm_bindgen]
pub struct Registry(publictransport::Registry);

#[wasm_bindgen]
impl Registry {
    /// A registry of the given networks, for example the bundled ones
    /// together with an application's own. If two share an ID, `get` finds
    /// the first.
    #[wasm_bindgen(constructor)]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "wasm-bindgen hands over a list by value"
    )]
    pub fn new(networks: Vec<Ts<Network>>) -> Result<Self, JsValue> {
        let networks = networks
            .iter()
            .map(|network| read("network", network))
            .collect::<Result<_, _>>()?;
        Ok(Self(publictransport::Registry::new(networks)))
    }

    /// The networks that ship with the package. Each was checked against
    /// its live server when it was added.
    #[must_use]
    pub fn bundled() -> Self {
        Self(publictransport::Registry::bundled())
    }

    /// All networks, in the order they were given.
    pub fn networks(&self) -> Result<Vec<Ts<Network>>, JsValue> {
        write_all(&self.0.networks())
    }

    /// The network with the given ID.
    pub fn get(&self, id: &str) -> Result<Option<Ts<Network>>, JsValue> {
        self.0.get(id).as_ref().map(write).transpose()
    }

    /// The networks with a coverage polygon that contains the position,
    /// each with the best tier of its coverage there. Those with the best
    /// tier come first. Coverage that an entry gives only as region codes
    /// is not found.
    pub fn covering(&self, position: &Ts<Coordinate>) -> Result<Vec<Ts<CoveringNetwork>>, JsValue> {
        write_all(&self.0.covering(read("position", position)?))
    }
}
