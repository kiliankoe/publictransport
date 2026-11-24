import type {
  Arrival,
  Coordinate,
  Departure,
  DeparturesQuery,
  HttpRequest,
  HttpResponse,
  Journey,
  Location,
  LocationQuery,
  NearbyQuery,
  Network,
  Place,
  Trips,
  TripsQuery,
} from "./generated/publictransport_wasm.js";
import {
  arrivalsRequest,
  departuresRequest,
  journeyRequest,
  locationsRequest,
  nearbyRequest,
  parseArrivals,
  parseDepartures,
  parseJourney,
  parseLocations,
  parseNearby,
  parseTrips,
  tripsRequest,
} from "./requests.js";

// Real answers stay below 1 MB. The limit keeps a broken or hostile server
// from filling the memory.
const LARGEST_BODY = 16 * 1024 * 1024;

/** The answer was larger than 16 MB, more than any real one, so reading it was stopped. */
export class ResponseTooLargeError extends Error {
  constructor() {
    super("the response is larger than 16 MB");
    this.name = "ResponseTooLargeError";
  }
}

/** What every call of a {@link Client} takes besides its query. */
export interface RequestOptions {
  /** Cancels the request when it aborts. `AbortSignal.timeout(30_000)` limits the time. */
  signal?: AbortSignal | undefined;
}

/** The time of a trip in a call of {@link Client.trips}: when to leave or when to arrive, not both. */
export type TripTimeOptions =
  | { departAt?: Date | undefined; arriveBy?: undefined }
  | { arriveBy?: Date | undefined; departAt?: undefined };

/**
 * Sends requests to public transport servers and waits for the answers.
 *
 * One client serves every {@link Network}, so create one and keep it. It
 * does not retry, limit its request rate or cache; those are decisions for
 * the application.
 *
 * ```ts
 * const client = new Client("my-app/1.0 (+https://example.org)");
 * const network = bundledNetwork("vvo-efa");
 * const [stop] = await client.locations(network, "Hauptbahnhof");
 * const board = stop ? await client.departures(network, stop) : [];
 * ```
 *
 * Every call rejects with a {@link PublicTransportError} for what the server
 * or the query got wrong, such as a {@link NotFoundError}, and with the
 * error of `fetch` if the HTTP exchange failed. The client does not follow
 * redirects: a moved server is a {@link MovedError}. A network or a query
 * of the wrong shape, such as a negative limit, is a `TypeError` before a
 * request is sent. A cancelled request rejects with the reason of its signal.
 */
export class Client {
  readonly #userAgent: string;
  readonly #fetch: typeof fetch;

  /**
   * @param userAgent Sent with every request. It should name the
   *   application, its version and a way to contact its authors, e.g.
   *   `my-app/1.0 (+https://example.org)`. Some servers require that. A
   *   browser may send its own instead.
   * @param options.fetch Sends the requests in place of the global `fetch`,
   *   for example through a proxy, which a page in a browser needs for most
   *   servers. It gets `redirect: "manual"` and the `signal`, and has to
   *   pass both on.
   * @throws TypeError if the user agent is blank or holds a character that
   *   an HTTP header cannot carry.
   */
  constructor(userAgent: string, options: { fetch?: typeof fetch } = {}) {
    if (userAgent.trim() === "") {
      throw new TypeError(
        "the user agent names the application to the servers, so it cannot be blank",
      );
    }
    // Throws here what every request would throw otherwise.
    new Headers({ "User-Agent": userAgent });
    this.#userAgent = userAgent;
    // The global one is looked up at each call.
    this.#fetch = options.fetch ?? ((input, init) => fetch(input, init));
  }

  /** Searches for locations with a query. */
  locations(network: Network, query: LocationQuery, options?: RequestOptions): Promise<Location[]>;
  /**
   * Searches for locations by name. The best match comes first.
   *
   * @param text What to search for, as a user typed it.
   * @param options The other fields of a {@link LocationQuery}: the `kinds`
   *   of location wanted and the largest number wanted as `limit`.
   */
  locations(
    network: Network,
    text: string,
    options?: Omit<LocationQuery, "text"> & RequestOptions,
  ): Promise<Location[]>;
  async locations(
    network: Network,
    query: string | LocationQuery,
    { signal, ...rest }: Omit<LocationQuery, "text"> & RequestOptions = {},
  ): Promise<Location[]> {
    const request = locationsRequest(
      network,
      typeof query === "string" ? { ...rest, text: query } : query,
    );
    return parseLocations(network, await this.#send(request, signal));
  }

  /** Finds the stops near a position with a query. */
  nearby(network: Network, query: NearbyQuery, options?: RequestOptions): Promise<Location[]>;
  /**
   * Finds the stops near a position. The nearest comes first, and
   * {@link distanceMeters} gives how far one is.
   *
   * @param position Where to look. A position off the globe rejects with an
   *   {@link InvalidQueryError}.
   * @param options The other fields of a {@link NearbyQuery}: how far to
   *   look as `radiusMeters`, 1000 m if left out, and the largest number
   *   wanted as `limit`, which servers treat as a guide. MOTIS has no
   *   radius and returns the nearest stops however far they are.
   */
  nearby(
    network: Network,
    position: Coordinate,
    options?: Omit<NearbyQuery, "position"> & RequestOptions,
  ): Promise<Location[]>;
  async nearby(
    network: Network,
    query: Coordinate | NearbyQuery,
    { signal, ...rest }: Omit<NearbyQuery, "position"> & RequestOptions = {},
  ): Promise<Location[]> {
    const request = nearbyRequest(
      network,
      "position" in query ? query : { ...rest, position: query },
    );
    return parseNearby(network, await this.#send(request, signal));
  }

  /** Lists departures with a query. */
  departures(
    network: Network,
    query: DeparturesQuery,
    options?: RequestOptions,
  ): Promise<Departure[]>;
  /**
   * Lists the next departures at a stop, ordered by expected time.
   *
   * @param stop A location from a search, a departure or a trip, or the ID
   *   of a stop on this network. A stop that another network returned works
   *   where this network can name it, which EFA, TRIAS and OJP can for stops
   *   with a global ID; otherwise the call rejects with an
   *   {@link InvalidQueryError}.
   * @param options The other fields of a {@link DeparturesQuery}: the `time`
   *   from which on departures are wanted, now if left out, and how many
   *   are wanted as `limit`. That is not a maximum: servers treat it as a
   *   guide and may return more or fewer.
   */
  departures(
    network: Network,
    stop: Place,
    options?: Omit<DeparturesQuery, "stop"> & RequestOptions,
  ): Promise<Departure[]>;
  async departures(
    network: Network,
    query: Place | DeparturesQuery,
    { signal, ...rest }: Omit<DeparturesQuery, "stop"> & RequestOptions = {},
  ): Promise<Departure[]> {
    const request = departuresRequest(network, departuresQuery(query, rest));
    return parseDepartures(network, await this.#send(request, signal));
  }

  /** Lists arrivals with a query, the one of departures. */
  arrivals(network: Network, query: DeparturesQuery, options?: RequestOptions): Promise<Arrival[]>;
  /**
   * Lists the next arrivals at a stop, ordered by expected time. An arrival
   * has an `origin`, where the vehicle comes from, in place of the
   * direction.
   *
   * @param stop A location or the ID of a stop on this network, as for
   *   {@link Client.departures}.
   * @param options The other fields of a {@link DeparturesQuery}: the `time`
   *   from which on arrivals are wanted, now if left out, and how many are
   *   wanted as `limit`. That is not a maximum: servers treat it as a guide
   *   and may return more or fewer.
   */
  arrivals(
    network: Network,
    stop: Place,
    options?: Omit<DeparturesQuery, "stop"> & RequestOptions,
  ): Promise<Arrival[]>;
  async arrivals(
    network: Network,
    query: Place | DeparturesQuery,
    { signal, ...rest }: Omit<DeparturesQuery, "stop"> & RequestOptions = {},
  ): Promise<Arrival[]> {
    const request = arrivalsRequest(network, departuresQuery(query, rest));
    return parseArrivals(network, await this.#send(request, signal));
  }

  /**
   * Fetches the run of one vehicle: all its stops with times and platforms.
   *
   * @param id The `journeyId` of a departure, an arrival or a leg that this
   *   network returned. It has no meaning on another network, and a server
   *   knows a run for about a day: after that the call rejects with a
   *   {@link NotFoundError}.
   */
  async journey(network: Network, id: string, { signal }: RequestOptions = {}): Promise<Journey> {
    return parseJourney(network, await this.#send(journeyRequest(network, id), signal));
  }

  /** Plans trips with a query. */
  trips(network: Network, query: TripsQuery, options?: RequestOptions): Promise<Trips>;
  /**
   * Plans trips between two places, in the order the server ranks them.
   * Each place is a location, the ID of a stop on this network or a
   * coordinate. A location that another network returned works as well.
   *
   * @param options When to leave as `departAt` or when to arrive as
   *   `arriveBy`, leaving now without either, and the other fields of a
   *   {@link TripsQuery}: how many trips are wanted as `limit`, which
   *   servers treat as a guide and not as a maximum, and `page`, the
   *   `earlier` or `later` of an answer, to continue that search with the
   *   same other arguments. A server that searches a window of time can
   *   answer with no trips and a reference to later ones, as at night:
   *   follow it before telling a user that there are none.
   */
  trips(
    network: Network,
    origin: Place,
    destination: Place,
    options?: Omit<TripsQuery, "origin" | "destination" | "time"> &
      TripTimeOptions &
      RequestOptions,
  ): Promise<Trips>;
  async trips(
    network: Network,
    origin: Place | TripsQuery,
    destination?: Place | RequestOptions,
    options?: Omit<TripsQuery, "origin" | "destination" | "time"> &
      TripTimeOptions &
      RequestOptions,
  ): Promise<Trips> {
    let query: TripsQuery;
    let signal: AbortSignal | undefined;
    if (typeof origin === "object" && "origin" in origin) {
      query = origin;
      signal = (destination as RequestOptions | undefined)?.signal;
    } else {
      const { departAt, arriveBy, signal: given, ...rest } = options ?? {};
      const time = departAt ? { departAt } : arriveBy && { arriveBy };
      query = { ...rest, origin, destination: destination as Place, time };
      signal = given;
    }
    return parseTrips(network, await this.#send(tripsRequest(network, query), signal));
  }

  async #send(request: HttpRequest, signal?: AbortSignal): Promise<HttpResponse> {
    const headers = new Headers({ "User-Agent": this.#userAgent });
    // After the user agent, so that one from the network takes its place.
    for (const { name, value } of request.headers) {
      headers.set(name, value);
    }
    // Called without a `this`, as a `fetch` expects it.
    const send = this.#fetch;
    const response = await send(request.url, {
      method: request.method,
      headers,
      // The type of the core allows shared memory, which the bytes are not in.
      body: (request.body as Uint8Array<ArrayBuffer> | undefined) ?? null,
      // The core gets the first answer of the server, whatever it is: it
      // reports a redirect as a moved endpoint.
      redirect: "manual",
      signal: signal ?? null,
    });
    // A browser hides the status and the headers of a redirect that is not followed.
    if (response.type === "opaqueredirect") {
      return { status: 302, headers: [], body: new Uint8Array() };
    }
    return {
      status: response.status,
      headers: Array.from(response.headers, ([name, value]) => ({ name, value })),
      body: await read(response),
    };
  }
}

/** The query of a call that names a stop, with the other fields, or gives the whole query. */
function departuresQuery(
  query: Place | DeparturesQuery,
  rest: Omit<DeparturesQuery, "stop">,
): DeparturesQuery {
  return typeof query === "object" && "stop" in query ? query : { ...rest, stop: query };
}

/** Reads the answer, up to the largest size. */
async function read(response: Response): Promise<Uint8Array> {
  const reader = response.body?.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (let chunk = await reader?.read(); chunk?.value; chunk = await reader?.read()) {
    size += chunk.value.length;
    if (size > LARGEST_BODY) {
      await reader?.cancel();
      throw new ResponseTooLargeError();
    }
    chunks.push(chunk.value);
  }
  const body = new Uint8Array(size);
  let end = 0;
  for (const chunk of chunks) {
    body.set(chunk, end);
    end += chunk.length;
  }
  return body;
}
