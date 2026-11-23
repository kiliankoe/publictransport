"""Clients that send the requests of the Rust library with httpx."""

from __future__ import annotations

from typing import TYPE_CHECKING

import httpx

from publictransport._uniffi import (
    Arrival,
    Coordinate,
    Departure,
    DeparturesQuery,
    Header,
    HttpRequest,
    HttpResponse,
    Location,
    LocationQuery,
    Network,
    Place,
    Trips,
    TripsQuery,
    TripTime,
)

if TYPE_CHECKING:
    from collections.abc import Sequence
    from datetime import datetime
    from types import TracebackType

    from typing_extensions import Self

    from publictransport._uniffi import LocationKind

TIMEOUT_SECONDS = 30.0
# Real answers stay below 1 MB. The limit keeps a broken or hostile server
# from filling the memory.
MAX_BODY_BYTES = 16 * 1024 * 1024


class ResponseTooLarge(Exception):  # noqa: N818
    """The answer was larger than any real one, so reading it was stopped."""

    def __init__(self) -> None:
        """Create the exception with its message."""
        super().__init__(
            f"the response is larger than {MAX_BODY_BYTES // (1024 * 1024)} MB"
        )


def _exactly_one(user_agent: str | None, http_client: object | None) -> None:
    if (user_agent is None) == (http_client is None):
        message = "pass either user_agent or http_client"
        raise ValueError(message)
    if user_agent is not None and not user_agent.strip():
        message = "user_agent is empty"
        raise ValueError(message)


def _no_options(name: str, *options: object) -> None:
    # By identity: the generated classes cannot be compared with other types.
    if any(option is not None for option in options):
        message = f"the {name} holds everything, so pass nothing next to it"
        raise TypeError(message)


def _place(place: Place | Location | Coordinate | str) -> Place:
    # The generated variants are not typed as a `Place`.
    if isinstance(place, Place):
        return place
    if isinstance(place, Location):
        return Place.LOCATION(place)  # pyright: ignore[reportReturnType]
    if isinstance(place, Coordinate):
        return Place.COORDINATE(place)  # pyright: ignore[reportReturnType]
    if isinstance(place, str):  # pyright: ignore[reportUnnecessaryIsInstance]
        return Place.ID(place)  # pyright: ignore[reportReturnType]
    message = f"a Location, a Coordinate, a Place or an ID is needed, not {place!r}"
    raise TypeError(message)


def _location_query(
    query: LocationQuery | str, kinds: Sequence[LocationKind], limit: int | None
) -> LocationQuery:
    if isinstance(query, LocationQuery):
        _no_options("LocationQuery", tuple(kinds) or None, limit)
        return query
    return LocationQuery(text=query, kinds=list(kinds), limit=limit)


def _departures_query(
    stop: DeparturesQuery | Place | Location | str,
    time: datetime | None,
    limit: int | None,
) -> DeparturesQuery:
    if isinstance(stop, DeparturesQuery):
        _no_options("DeparturesQuery", time, limit)
        return stop
    return DeparturesQuery(stop=_place(stop), time=time, limit=limit)


def _trips_query(  # noqa: PLR0913
    origin: TripsQuery | Place | Location | Coordinate | str,
    destination: Place | Location | Coordinate | str | None,
    *,
    depart_at: datetime | None,
    arrive_by: datetime | None,
    limit: int | None,
    page: str | None,
) -> TripsQuery:
    if isinstance(origin, TripsQuery):
        _no_options("TripsQuery", destination, depart_at, arrive_by, limit, page)
        return origin
    if destination is None:
        message = "trips need a destination"
        raise TypeError(message)
    if depart_at is not None and arrive_by is not None:
        message = "pass depart_at or arrive_by, not both"
        raise ValueError(message)
    time = None
    if depart_at is not None:
        time = TripTime.DEPART_AT(depart_at)
    if arrive_by is not None:
        time = TripTime.ARRIVE_BY(arrive_by)
    return TripsQuery(
        origin=_place(origin),
        destination=_place(destination),
        time=time,  # pyright: ignore[reportArgumentType]
        limit=limit,
        page=page,
    )


def _headers(request: HttpRequest) -> list[tuple[str, str]]:
    return [(header.name, header.value) for header in request.headers]


def _append(body: bytearray, chunk: bytes) -> None:
    if len(body) + len(chunk) > MAX_BODY_BYTES:
        raise ResponseTooLarge
    body += chunk


def _response(response: httpx.Response, body: bytearray) -> HttpResponse:
    return HttpResponse(
        status=response.status_code,
        headers=[
            Header(name=name, value=value)
            for name, value in response.headers.multi_items()
        ],
        body=bytes(body),
    )


class Client:
    """Send requests to public transport servers and wait for the answers.

    One client serves every `Network` and shares its connections between
    them. Use it as a context manager or call `close` when done.

    Failures the server or the query cause are raised as `Error` subclasses,
    e.g. `Error.NotFound`. Failures of the HTTP exchange are raised as httpx
    raises them, e.g. `httpx.ConnectError` or `httpx.TimeoutException`.

    The client does not retry, limit its request rate or cache.
    """

    def __init__(
        self,
        user_agent: str | None = None,
        *,
        http_client: httpx.Client | None = None,
    ) -> None:
        """Pass either `user_agent` or `http_client`.

        `user_agent` is sent with every request. It should name the
        application, its version and a way to contact its authors, e.g.
        `my-app/1.0 (+https://example.org)`. Some servers, such as
        Transitous, require that. Requests time out after 30 seconds.

        `http_client` is for a caller who configures httpx, e.g. with a
        proxy. It must not follow redirects, so that a moved endpoint is
        raised as `Error.Moved`, and it should set a `User-Agent`. The client
        does not close it.

        Raises `ValueError` unless exactly one of the two is given, and for
        an empty `user_agent`.
        """
        _exactly_one(user_agent, http_client)
        self._owned = http_client is None
        self._http = http_client or httpx.Client(
            headers={"User-Agent": user_agent or ""},
            timeout=TIMEOUT_SECONDS,
            # The library reports a redirect as a moved endpoint.
            follow_redirects=False,
        )

    def locations(
        self,
        network: Network,
        query: LocationQuery | str,
        *,
        kinds: Sequence[LocationKind] = (),
        limit: int | None = None,
    ) -> list[Location]:
        """Search for locations by name. The best match comes first.

        `query` is the text to search for. `kinds` narrows the search to
        kinds of location and `limit` is the largest number wanted, which
        is not sent to EFA servers: they return every match. A
        `LocationQuery` carries the same and takes the place of all three.
        """
        request = network.locations_request(_location_query(query, kinds, limit))
        return network.parse_locations(self._send(request))

    def departures(
        self,
        network: Network,
        stop: DeparturesQuery | Place | Location | str,
        *,
        time: datetime | None = None,
        limit: int | None = None,
    ) -> list[Departure]:
        """List the next departures at a stop, ordered by expected time.

        `stop` is a location or the ID of a stop on this network. A stop
        that another network returned works where this network can name it,
        which EFA, TRIAS and OJP can for stops with a global ID; otherwise
        the call raises `Error.InvalidQuery`. `time` is when the list starts,
        now if left out, and `limit` how many departures are wanted. A
        `DeparturesQuery` carries the same and takes the place of all three.

        `limit` is not a maximum: servers treat it as a guide and may return
        more or fewer.
        """
        request = network.departures_request(_departures_query(stop, time, limit))
        return network.parse_departures(self._send(request))

    def arrivals(
        self,
        network: Network,
        stop: DeparturesQuery | Place | Location | str,
        *,
        time: datetime | None = None,
        limit: int | None = None,
    ) -> list[Arrival]:
        """List the next arrivals at a stop, ordered by expected time.

        The arguments are those of `departures`. An arrival has an `origin`,
        where the vehicle comes from, in place of the direction. It is
        `None` where the server does not say, as with OpenTripPlanner.
        """
        request = network.arrivals_request(_departures_query(stop, time, limit))
        return network.parse_arrivals(self._send(request))

    def trips(  # noqa: PLR0913
        self,
        network: Network,
        origin: TripsQuery | Place | Location | Coordinate | str,
        destination: Place | Location | Coordinate | str | None = None,
        *,
        depart_at: datetime | None = None,
        arrive_by: datetime | None = None,
        limit: int | None = None,
        page: str | None = None,
    ) -> Trips:
        """Plan trips between two places, in the order the server ranks them.

        Each end is a location, also one that another network returned, a
        coordinate, or an ID on this network. A location this network
        cannot name by ID is sent as its coordinate. Pass `depart_at` or
        `arrive_by` for another time than now, `limit` for how many trips
        are wanted, and `page` with `earlier` or `later` of an answer to
        continue that search with the same other arguments. A `TripsQuery`
        carries the same and takes the place of all of them.

        `limit` is not a maximum: servers treat it as a guide and may return
        more or fewer.
        """
        query = _trips_query(
            origin,
            destination,
            depart_at=depart_at,
            arrive_by=arrive_by,
            limit=limit,
            page=page,
        )
        return network.parse_trips(self._send(network.trips_request(query)))

    def close(self) -> None:
        """Close the connections, unless the caller passed `http_client`."""
        if self._owned:
            self._http.close()

    def _send(self, request: HttpRequest) -> HttpResponse:
        with self._http.stream(
            request.method.name,
            request.url,
            headers=_headers(request),
            content=request.body,
        ) as response:
            body = bytearray()
            for chunk in response.iter_bytes():
                _append(body, chunk)
            return _response(response, body)

    def __enter__(self) -> Self:
        """Return the client itself."""
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        """Close the client."""
        self.close()


class AsyncClient:
    """The same as `Client`, with coroutines for asyncio.

    `http_client` is an `httpx.AsyncClient` here, and `aclose` takes the
    place of `close`.
    """

    def __init__(
        self,
        user_agent: str | None = None,
        *,
        http_client: httpx.AsyncClient | None = None,
    ) -> None:
        """Pass either `user_agent` or `http_client`, as for `Client`."""
        _exactly_one(user_agent, http_client)
        self._owned = http_client is None
        self._http = http_client or httpx.AsyncClient(
            headers={"User-Agent": user_agent or ""},
            timeout=TIMEOUT_SECONDS,
            follow_redirects=False,
        )

    async def locations(
        self,
        network: Network,
        query: LocationQuery | str,
        *,
        kinds: Sequence[LocationKind] = (),
        limit: int | None = None,
    ) -> list[Location]:
        """Search for locations by name. The best match comes first.

        `query` is the text to search for. `kinds` narrows the search to
        kinds of location and `limit` is the largest number wanted, which
        is not sent to EFA servers: they return every match. A
        `LocationQuery` carries the same and takes the place of all three.
        """
        request = network.locations_request(_location_query(query, kinds, limit))
        return network.parse_locations(await self._send(request))

    async def departures(
        self,
        network: Network,
        stop: DeparturesQuery | Place | Location | str,
        *,
        time: datetime | None = None,
        limit: int | None = None,
    ) -> list[Departure]:
        """List the next departures at a stop, ordered by expected time.

        `stop` is a location or the ID of a stop on this network. A stop
        that another network returned works where this network can name it,
        which EFA, TRIAS and OJP can for stops with a global ID; otherwise
        the call raises `Error.InvalidQuery`. `time` is when the list starts,
        now if left out, and `limit` how many departures are wanted. A
        `DeparturesQuery` carries the same and takes the place of all three.

        `limit` is not a maximum: servers treat it as a guide and may return
        more or fewer.
        """
        request = network.departures_request(_departures_query(stop, time, limit))
        return network.parse_departures(await self._send(request))

    async def arrivals(
        self,
        network: Network,
        stop: DeparturesQuery | Place | Location | str,
        *,
        time: datetime | None = None,
        limit: int | None = None,
    ) -> list[Arrival]:
        """List the next arrivals at a stop, ordered by expected time.

        The arguments are those of `departures`. An arrival has an `origin`,
        where the vehicle comes from, in place of the direction. It is
        `None` where the server does not say, as with OpenTripPlanner.
        """
        request = network.arrivals_request(_departures_query(stop, time, limit))
        return network.parse_arrivals(await self._send(request))

    async def trips(  # noqa: PLR0913
        self,
        network: Network,
        origin: TripsQuery | Place | Location | Coordinate | str,
        destination: Place | Location | Coordinate | str | None = None,
        *,
        depart_at: datetime | None = None,
        arrive_by: datetime | None = None,
        limit: int | None = None,
        page: str | None = None,
    ) -> Trips:
        """Plan trips between two places, in the order the server ranks them.

        Each end is a location, also one that another network returned, a
        coordinate, or an ID on this network. A location this network
        cannot name by ID is sent as its coordinate. Pass `depart_at` or
        `arrive_by` for another time than now, `limit` for how many trips
        are wanted, and `page` with `earlier` or `later` of an answer to
        continue that search with the same other arguments. A `TripsQuery`
        carries the same and takes the place of all of them.

        `limit` is not a maximum: servers treat it as a guide and may return
        more or fewer.
        """
        query = _trips_query(
            origin,
            destination,
            depart_at=depart_at,
            arrive_by=arrive_by,
            limit=limit,
            page=page,
        )
        return network.parse_trips(await self._send(network.trips_request(query)))

    async def aclose(self) -> None:
        """Close the connections, unless the caller passed `http_client`."""
        if self._owned:
            await self._http.aclose()

    async def _send(self, request: HttpRequest) -> HttpResponse:
        async with self._http.stream(
            request.method.name,
            request.url,
            headers=_headers(request),
            content=request.body,
        ) as response:
            body = bytearray()
            async for chunk in response.aiter_bytes():
                _append(body, chunk)
            return _response(response, body)

    async def __aenter__(self) -> Self:
        """Return the client itself."""
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        """Close the client."""
        await self.aclose()
