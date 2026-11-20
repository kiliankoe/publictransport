import type { Thrown } from "./generated/publictransport_wasm.js";

/**
 * Why a request could not be built or an answer could not be used. Each
 * reason is a subclass. The `message` is a sentence in English.
 */
export class PublicTransportError extends Error {
  // Written out in every class, because a bundler shortens the names of classes.
  override name = "PublicTransportError";

  /** What is missing or wrong, or what the server said, where the error has more than its kind to tell. */
  readonly detail: string | undefined;

  constructor(message: string, detail?: string) {
    super(message);
    this.detail = detail;
  }
}

/** The network lacks something the request needs, such as a time zone, or its description cannot be read. */
export class InvalidNetworkError extends PublicTransportError {
  override name = "InvalidNetworkError";
}

/** The query holds a value no request of the protocol can carry, or a place the network cannot name. */
export class InvalidQueryError extends PublicTransportError {
  override name = "InvalidQueryError";
}

/** The network's server is known not to answer this kind of request, or the network lacks what it needs. */
export class UnsupportedError extends PublicTransportError {
  override name = "UnsupportedError";
}

/** The network does not know the stop the query names, or knows no stop near a place of a trip. */
export class NotFoundError extends PublicTransportError {
  override name = "NotFoundError";
}

/** The server redirected the request, so the endpoint has moved. */
export class MovedError extends PublicTransportError {
  override name = "MovedError";

  /** The value of the `Location` header. A browser does not show it. */
  readonly location: string | undefined;

  constructor(message: string, location?: string) {
    super(message);
    this.location = location;
  }
}

/** The server refuses this client (status 403). Trying again does not help. */
export class BlockedError extends PublicTransportError {
  override name = "BlockedError";
}

/** The client sent too many requests (status 429). Trying again later can help. */
export class RateLimitedError extends PublicTransportError {
  override name = "RateLimitedError";
}

/** The server rejected the credentials (status 401, or the code `AUTH` in a HAFAS answer). */
export class UnauthorizedError extends PublicTransportError {
  override name = "UnauthorizedError";
}

/** The server failed (status 5xx). */
export class UnavailableError extends PublicTransportError {
  override name = "UnavailableError";
}

/**
 * The server reported any other failure: an HTTP status that no other class
 * covers, or an error inside a successful answer, as TRIAS, OJP and HAFAS
 * send them.
 */
export class ServerError extends PublicTransportError {
  override name = "ServerError";

  /** The HTTP status code. It is 200 for an error inside a successful answer. */
  readonly status: number;

  constructor(message: string, detail: string, status: number) {
    super(message, detail);
    this.status = status;
  }
}

/** The answer was not in the format the protocol defines. */
export class ParseError extends PublicTransportError {
  override name = "ParseError";
}

function translated({ message, failure }: Thrown): PublicTransportError {
  switch (failure.type) {
    case "invalidNetwork":
      return new InvalidNetworkError(message, failure.message);
    case "invalidQuery":
      return new InvalidQueryError(message, failure.message);
    case "unsupported":
      return new UnsupportedError(message, failure.message);
    case "notFound":
      return new NotFoundError(message, failure.message);
    case "moved":
      return new MovedError(message, failure.location);
    case "blocked":
      return new BlockedError(message);
    case "rateLimited":
      return new RateLimitedError(message);
    case "unauthorized":
      return new UnauthorizedError(message);
    case "unavailable":
      return new UnavailableError(message);
    case "server":
      return new ServerError(message, failure.message, failure.status);
    case "parse":
      return new ParseError(message, failure.message);
  }
}

/**
 * The function of the WebAssembly module, throwing a
 * {@link PublicTransportError} where the module throws a plain value.
 */
export function throwing<Arguments extends unknown[], Result>(
  call: (...values: Arguments) => Result,
): (...values: Arguments) => Result {
  return (...values) => {
    try {
      return call(...values);
    } catch (thrown) {
      // An argument of the wrong shape is an `Error` already.
      throw thrown instanceof Error ? thrown : translated(thrown as Thrown);
    }
  };
}
