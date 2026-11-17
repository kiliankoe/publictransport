"""Interface tests for `Client` and `AsyncClient`.

Every request goes to an `httpx.MockTransport`. Building requests and parsing
answers is the Rust library's job and tested there, so the tests compare with
what the library itself returns.
"""

from __future__ import annotations

import asyncio
import http.server
import threading
from collections.abc import AsyncIterator, Callable, Iterator
from datetime import datetime, timezone
from pathlib import Path

import httpx
import pytest
from publictransport import (
    AsyncClient,
    Client,
    DeparturesQuery,
    Error,
    Header,
    HttpResponse,
    LocationQuery,
    Network,
    Place,
    Protocol,
    ResponseTooLarge,
    TripsQuery,
)

FIXTURES = Path(__file__).parents[3] / "crates/core/tests/fixtures"
USER_AGENT = "publictransport-tests/1.0"
MEGABYTE = b" " * (1024 * 1024)

EFA = Network(
    id="vvo",
    name="VVO",
    endpoint="https://efa.example/std3",
    protocol=Protocol.EFA,
    timezone="Europe/Berlin",
)
MOTIS = Network(
    id="transitous",
    name="Transitous",
    endpoint="https://motis.example/api",
    protocol=Protocol.MOTIS,
)
# Nothing listens here, so a request that should not be sent fails locally.
UNREACHABLE = Network(
    id="none", name="None", endpoint="http://127.0.0.1:1", protocol=Protocol.MOTIS
)
LOCATIONS = LocationQuery(text="Hauptbahnhof")

Handler = Callable[[httpx.Request], httpx.Response]
# Runs one operation ("locations", "departures" or "trips") of a client whose
# requests the handler answers.
Call = Callable[[Handler, str, Network, object], object]


def blocking(
    handler: Handler, operation: str, network: Network, query: object
) -> object:
    with httpx.Client(
        transport=httpx.MockTransport(handler), headers={"User-Agent": USER_AGENT}
    ) as http:
        return getattr(Client(http_client=http), operation)(network, query)


def asynchronous(
    handler: Handler, operation: str, network: Network, query: object
) -> object:
    async def run() -> object:
        async with httpx.AsyncClient(
            transport=httpx.MockTransport(handler), headers={"User-Agent": USER_AGENT}
        ) as http:
            client = AsyncClient(http_client=http)
            return await getattr(client, operation)(network, query)

    return asyncio.run(run())


@pytest.fixture(params=[blocking, asynchronous])
def call(request: pytest.FixtureRequest) -> Call:
    return request.param


class TooLarge(httpx.SyncByteStream, httpx.AsyncByteStream):
    """17 MB without a `Content-Length`, as a hostile server would send it."""

    def __iter__(self) -> Iterator[bytes]:
        return iter([MEGABYTE] * 17)

    async def __aiter__(self) -> AsyncIterator[bytes]:
        for chunk in self:
            yield chunk


@pytest.mark.parametrize(
    ("network", "operation", "query", "fixture"),
    [
        (EFA, "locations", LOCATIONS, "efa/vvo-stopfinder.json"),
        (
            MOTIS,
            "departures",
            DeparturesQuery(stop_id="de-DELFI_de:14612:28"),
            "motis/transitous-stoptimes.json",
        ),
        (
            EFA,
            "trips",
            # The generated `Place.ID` is not typed as a `Place`.
            TripsQuery(
                origin=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
                destination=Place.ID("de:14612:13"),  # pyright: ignore[reportArgumentType]
            ),
            "efa/vvo-trip.json",
        ),
    ],
)
def test_sends_the_library_request_and_returns_the_parsed_answer(
    call: Call, network: Network, operation: str, query: object, fixture: str
) -> None:
    body = (FIXTURES / fixture).read_bytes()
    request = getattr(network, f"{operation}_request")(query)
    expected = getattr(network, f"parse_{operation}")(
        HttpResponse(status=200, headers=[], body=body)
    )
    # A trips answer holds its trips next to the paging references.
    assert getattr(expected, "trips", expected)
    sent: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request)
        return httpx.Response(200, content=body)

    assert call(handler, operation, network, query) == expected
    assert [(r.method, r.url) for r in sent] == [
        (request.method.name, httpx.URL(request.url))
    ]
    # The caller's httpx client is used as it is.
    assert sent[0].headers["User-Agent"] == USER_AGENT


@pytest.mark.parametrize(
    ("status", "error", "fields"),
    [
        (301, Error.Moved, {"location": "https://new.example/std3"}),
        (429, Error.RateLimited, {}),
        (418, Error.Server, {"status": 418, "message": "teapot"}),
    ],
)
def test_status_headers_and_body_reach_the_library(
    call: Call, status: int, error: type[Exception], fields: dict[str, object]
) -> None:
    sent: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request)
        return httpx.Response(
            status, headers={"Location": "https://new.example/std3"}, content=b"teapot"
        )

    with pytest.raises(error) as raised:
        call(handler, "locations", EFA, LOCATIONS)
    assert vars(raised.value) == fields
    assert len(sent) == 1


def test_rejected_query_sends_no_request(call: Call) -> None:
    network = Network(
        id="vvo", name="VVO", endpoint="https://efa.example/std3", protocol=Protocol.EFA
    )
    query = DeparturesQuery(
        stop_id="de:14612:28", time=datetime(2026, 1, 1, tzinfo=timezone.utc)
    )
    sent: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request)
        return httpx.Response(200, content=b"{}")

    with pytest.raises(Error.InvalidNetwork):
        call(handler, "departures", network, query)
    assert sent == []


def test_http_failure_is_raised_as_httpx_raises_it(call: Call) -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        message = "refused"
        raise httpx.ConnectError(message, request=request)

    with pytest.raises(httpx.ConnectError):
        call(handler, "locations", EFA, LOCATIONS)


def test_too_large_answer_is_refused(call: Call) -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        return httpx.Response(200, stream=TooLarge())

    with pytest.raises(ResponseTooLarge):
        call(handler, "locations", EFA, LOCATIONS)


@pytest.mark.parametrize(
    "build",
    [
        Client,
        lambda: Client(USER_AGENT, http_client=httpx.Client()),
        AsyncClient,
        lambda: AsyncClient(USER_AGENT, http_client=httpx.AsyncClient()),
    ],
)
def test_constructor_takes_exactly_one_argument(build: Callable[[], object]) -> None:
    with pytest.raises(ValueError):  # noqa: PT011 the message is not part of the contract
        build()


def test_callers_http_client_stays_open() -> None:
    http = httpx.Client()
    with Client(http_client=http):
        pass
    assert not http.is_closed

    async def run() -> bool:
        http = httpx.AsyncClient()
        async with AsyncClient(http_client=http):
            pass
        return http.is_closed

    assert not asyncio.run(run())


def test_own_connections_are_closed() -> None:
    # httpx refuses to send on a closed client before it opens a connection.
    with Client(USER_AGENT) as client:
        pass
    with pytest.raises(RuntimeError, match="closed"):
        client.locations(UNREACHABLE, LOCATIONS)

    async def run() -> None:
        async with AsyncClient(USER_AGENT) as client:
            pass
        await client.locations(UNREACHABLE, LOCATIONS)

    with pytest.raises(RuntimeError, match="closed"):
        asyncio.run(run())


def test_empty_user_agent_is_refused() -> None:
    with pytest.raises(ValueError, match="user_agent"):
        Client(" ")
    with pytest.raises(ValueError, match="user_agent"):
        AsyncClient("")


def test_own_client_identifies_itself_and_does_not_follow_redirects() -> None:
    seen: list[tuple[str, str | None]] = []

    class Redirect(http.server.BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            seen.append((self.path.split("?")[0], self.headers.get("User-Agent")))
            self.send_response(301)
            self.send_header("Location", "/elsewhere")
            self.send_header("Content-Length", "0")
            self.end_headers()

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            pass

    async def run(network: Network) -> None:
        async with AsyncClient(USER_AGENT) as client:
            await client.locations(network, LOCATIONS)

    with http.server.HTTPServer(("127.0.0.1", 0), Redirect) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        network = Network(
            id="local",
            name="Local",
            endpoint=f"http://127.0.0.1:{server.server_port}/api",
            protocol=Protocol.MOTIS,
        )
        try:
            with Client(USER_AGENT) as client, pytest.raises(Error.Moved):
                client.locations(network, LOCATIONS)
            with pytest.raises(Error.Moved):
                asyncio.run(run(network))
        finally:
            server.shutdown()

    assert seen == [("/api/v1/geocode", USER_AGENT)] * 2


def test_request_body_and_headers_arrive_as_the_library_built_them(call: Call) -> None:
    # TRIAS is the protocol whose requests have a body and a header of their
    # own, next to the one of the network.
    network = Network(
        id="trias",
        name="TRIAS",
        endpoint="https://trias.example/trias",
        protocol=Protocol.TRIAS,
        requestor_ref="OpenService",
        headers=[Header(name="X-Api-Key", value="secret")],
    )
    built = network.locations_request(LOCATIONS)
    sent: list[httpx.Request] = []

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request)
        body = (FIXTURES / "trias/vvo-locations.xml").read_bytes()
        return httpx.Response(200, content=body)

    found = call(handler, "locations", network, LOCATIONS)

    assert isinstance(found, list)
    assert [(r.method, r.content) for r in sent] == [("POST", built.body)]
    assert sorted(sent[0].headers[h.name] for h in built.headers) == [
        "application/xml",
        "secret",
    ]
