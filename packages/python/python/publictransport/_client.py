"""Clients that send the requests of the Rust library with httpx."""

from __future__ import annotations

from typing import TYPE_CHECKING

import httpx

from publictransport._uniffi import (
    Departure,
    DeparturesQuery,
    Header,
    HttpRequest,
    HttpResponse,
    Location,
    LocationQuery,
    Network,
    Trips,
    TripsQuery,
)

if TYPE_CHECKING:
    from types import TracebackType

    from typing_extensions import Self

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

    def locations(self, network: Network, query: LocationQuery) -> list[Location]:
        """Search for locations by name. The best match comes first."""
        response = self._send(network.locations_request(query))
        return network.parse_locations(response)

    def departures(self, network: Network, query: DeparturesQuery) -> list[Departure]:
        """List the next departures at a stop, ordered by expected time."""
        response = self._send(network.departures_request(query))
        return network.parse_departures(response)

    def trips(self, network: Network, query: TripsQuery) -> Trips:
        """Plan trips between two places, in the order the server ranks them."""
        response = self._send(network.trips_request(query))
        return network.parse_trips(response)

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

    async def locations(self, network: Network, query: LocationQuery) -> list[Location]:
        """Search for locations by name. The best match comes first."""
        response = await self._send(network.locations_request(query))
        return network.parse_locations(response)

    async def departures(
        self, network: Network, query: DeparturesQuery
    ) -> list[Departure]:
        """List the next departures at a stop, ordered by expected time."""
        response = await self._send(network.departures_request(query))
        return network.parse_departures(response)

    async def trips(self, network: Network, query: TripsQuery) -> Trips:
        """Plan trips between two places, in the order the server ranks them."""
        response = await self._send(network.trips_request(query))
        return network.parse_trips(response)

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
