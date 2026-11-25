// The functions of the WebAssembly module that can fail with an error of
// the library, throwing it as a `PublicTransportError`.

import { throwing } from "./errors.js";
import {
  arrivalsRequest as arrivalsRequestOrThrown,
  departuresRequest as departuresRequestOrThrown,
  journeyRequest as journeyRequestOrThrown,
  locationsRequest as locationsRequestOrThrown,
  nearbyRequest as nearbyRequestOrThrown,
  networkFromJson as networkFromJsonOrThrown,
  parseArrivals as parseArrivalsOrThrown,
  parseDepartures as parseDeparturesOrThrown,
  parseJourney as parseJourneyOrThrown,
  parseLocations as parseLocationsOrThrown,
  parseNearby as parseNearbyOrThrown,
  parseTrips as parseTripsOrThrown,
  tripsRequest as tripsRequestOrThrown,
} from "./generated/publictransport_wasm.js";

/**
 * Builds the request that searches for locations by name, for a caller who
 * sends it with an HTTP stack of their own. That stack sets a `User-Agent`,
 * does not follow redirects and passes on every answer, whatever its
 * status, to {@link parseLocations}.
 */
export const locationsRequest = throwing(locationsRequestOrThrown);

/**
 * Reads the answer to the {@link locationsRequest} of a query. The best match
 * comes first, and the list is cut to the limit of the query.
 */
export const parseLocations = throwing(parseLocationsOrThrown);

/** Builds the request for the stops near a position. */
export const nearbyRequest = throwing(nearbyRequestOrThrown);

/**
 * Reads the answer to the {@link nearbyRequest} of a query. The nearest stop
 * comes first. A stop outside the circle of the query is left out, and the
 * rest is cut to the limit of the query.
 */
export const parseNearby = throwing(parseNearbyOrThrown);

/** Builds the request for the next departures at a stop. */
export const departuresRequest = throwing(departuresRequestOrThrown);

/**
 * Reads the answer to the {@link departuresRequest} of a query. The
 * departures are ordered by expected time. Those expected before the time of
 * the query are left out, and the rest is cut to its limit.
 */
export const parseDepartures = throwing(parseDeparturesOrThrown);

/** Builds the request for the next arrivals at a stop, from the query that {@link departuresRequest} takes. */
export const arrivalsRequest = throwing(arrivalsRequestOrThrown);

/**
 * Reads the answer to the {@link arrivalsRequest} of a query. The arrivals
 * are ordered by expected time, and left out and cut as in
 * {@link parseDepartures}.
 */
export const parseArrivals = throwing(parseArrivalsOrThrown);

/**
 * Builds the request for the run of one vehicle, from the `journeyId` of a
 * departure, an arrival or a leg that the network returned.
 */
export const journeyRequest = throwing(journeyRequestOrThrown);

/** Reads the answer to a {@link journeyRequest}. The stops are in the order of travel. */
export const parseJourney = throwing(parseJourneyOrThrown);

/** Builds the request for trips between two places. */
export const tripsRequest = throwing(tripsRequestOrThrown);

/**
 * Reads the answer to the {@link tripsRequest} of a query. The trips are in
 * the order the server ranks them. On EFA, TRIAS and OJP the query tells
 * which trips the page before already had, and those are left out.
 */
export const parseTrips = throwing(parseTripsOrThrown);

/**
 * Reads a network from an entry in the JSON format of the
 * [transport-apis](https://github.com/public-transport/transport-apis)
 * repository. `id` is the name of the entry's file without `.json`.
 */
export const networkFromJson = throwing(networkFromJsonOrThrown);
