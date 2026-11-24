/**
 * A client for public transport APIs: EFA, MOTIS, TRIAS, OJP, HAFAS and
 * OpenTripPlanner.
 *
 * The model types and the functions on them are generated from the Rust
 * library of the same name, which builds the requests and parses the
 * answers as a WebAssembly module. {@link Client} sends the requests with
 * `fetch`.
 *
 * @module
 */

export * from "./client.js";
export {
  BlockedError,
  InvalidNetworkError,
  InvalidQueryError,
  MovedError,
  NotFoundError,
  ParseError,
  PublicTransportError,
  RateLimitedError,
  ServerError,
  UnauthorizedError,
  UnavailableError,
  UnsupportedError,
} from "./errors.js";
// The types, and the functions that cannot fail. Those in requests.ts take
// the place of the ones that can.
export type * from "./generated/publictransport_wasm.js";
export {
  arrival,
  bundledNetwork,
  delaySeconds,
  departure,
  distanceMeters,
  durationSeconds,
  expectedTime,
  localExpectedTime,
  localScheduledTime,
  localTime,
  place,
  platform,
  Registry,
  sameStation,
  stationIds,
  stopId,
  stopoverArrival,
  stopoverDeparture,
  tierAt,
  transfers,
} from "./generated/publictransport_wasm.js";
export {
  arrivalsRequest,
  departuresRequest,
  journeyRequest,
  locationsRequest,
  nearbyRequest,
  networkFromJson,
  parseArrivals,
  parseDepartures,
  parseJourney,
  parseLocations,
  parseNearby,
  parseTrips,
  tripsRequest,
} from "./requests.js";
