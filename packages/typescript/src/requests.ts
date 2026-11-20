// The functions of the WebAssembly module that can fail with an error of
// the library, throwing it as a `PublicTransportError`.

import { throwing } from "./errors.js";
import {
  departuresRequest as departuresRequestOrThrown,
  locationsRequest as locationsRequestOrThrown,
  networkFromJson as networkFromJsonOrThrown,
  parseDepartures as parseDeparturesOrThrown,
  parseLocations as parseLocationsOrThrown,
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

/** Reads the answer to a {@link locationsRequest}. The best match comes first. */
export const parseLocations = throwing(parseLocationsOrThrown);

/** Builds the request for the next departures at a stop. */
export const departuresRequest = throwing(departuresRequestOrThrown);

/** Reads the answer to a {@link departuresRequest}. The departures are ordered by expected time. */
export const parseDepartures = throwing(parseDeparturesOrThrown);

/** Builds the request for trips between two places. */
export const tripsRequest = throwing(tripsRequestOrThrown);

/** Reads the answer to a {@link tripsRequest}. The trips are in the order the server ranks them. */
export const parseTrips = throwing(parseTripsOrThrown);

/**
 * Reads a network from an entry in the JSON format of the
 * [transport-apis](https://github.com/public-transport/transport-apis)
 * repository. `id` is the name of the entry's file without `.json`.
 */
export const networkFromJson = throwing(networkFromJsonOrThrown);
