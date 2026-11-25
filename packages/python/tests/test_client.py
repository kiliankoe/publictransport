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
    Coordinate,
    DeparturesQuery,
    Error,
    GlobalId,
    Header,
    HttpResponse,
    IdScheme,
    Location,
    LocationKind,
    LocationQuery,
    NearbyQuery,
    Network,
    Place,
    Protocol,
    ResponseTooLarge,
    TripsQuery,
    TripTime,
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
# The run of a departure in the answer that was recorded with the run.
JOURNEY_ID = next(
    departure.journey_id
    for departure in EFA.parse_departures(
        DeparturesQuery(stop=Place.ID("x")),  # pyright: ignore[reportArgumentType]
        HttpResponse(
            status=200,
            headers=[],
            body=(FIXTURES / "efa/vvo-journey-departure.json").read_bytes(),
        ),
    )
    if departure.journey_id
)

Handler = Callable[[httpx.Request], httpx.Response]
# Runs one operation ("locations", "nearby", "departures", "arrivals",
# "journey" or "trips") of a client whose requests the handler answers.
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
            EFA,
            "nearby",
            NearbyQuery(
                position=Coordinate(latitude=51.04, longitude=13.7325),
                radius_meters=500,
                limit=5,
            ),
            "efa/vvo-nearby.json",
        ),
        (
            MOTIS,
            "departures",
            DeparturesQuery(
                stop=Place.ID("de-DELFI_de:14612:28"),  # pyright: ignore[reportArgumentType]
            ),
            "motis/transitous-stoptimes.json",
        ),
        (
            EFA,
            "arrivals",
            DeparturesQuery(
                stop=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
            ),
            "efa/vvo-arrivals.json",
        ),
        (EFA, "journey", JOURNEY_ID, "efa/vvo-journey.json"),
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
    response = HttpResponse(status=200, headers=[], body=body)
    parse = getattr(network, f"parse_{operation}")
    # All but a journey are parsed for the query that was sent.
    expected = parse(response) if operation == "journey" else parse(query, response)
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
        (429, Error.RateLimited, {"retry_after_seconds": 120}),
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
            status,
            headers={"Location": "https://new.example/std3", "Retry-After": "120"},
            content=b"teapot",
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
        stop=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
        time=datetime(2026, 1, 1, tzinfo=timezone.utc),
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


TIME = datetime(2026, 11, 16, 13, 24, tzinfo=timezone.utc)
DRESDEN = Coordinate(latitude=51.04, longitude=13.73)


def foreign(kind: LocationKind) -> Location:
    """Return a location that another network returned."""
    return Location(
        id="8010085",
        global_ids=[GlobalId(scheme=IdScheme.IFOPT, value="de:14612:28:3:2")],
        kind=kind,
        name="Dresden Hbf",
        locality=None,
        coordinate=None,
        timezone=None,
        network="other",
    )


FOREIGN_STOP = foreign(LocationKind.STOP)
EFA_FIXTURES = {
    "locations": "efa/vvo-stopfinder.json",
    "departures": "efa/vvo-departures.json",
    "trips": "efa/vvo-trip.json",
}
# The reference to the trips after those of the fixture.
LATER = EFA.parse_trips(
    TripsQuery(
        origin=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
        destination=Place.ID("de:14612:13"),  # pyright: ignore[reportArgumentType]
    ),
    HttpResponse(
        status=200, headers=[], body=(FIXTURES / EFA_FIXTURES["trips"]).read_bytes()
    ),
).later


def sent_by(operation: str, *args: object, **options: object) -> list[httpx.Request]:
    """Return the requests one plain call of the blocking client sends to EFA."""
    sent: list[httpx.Request] = []
    body = (FIXTURES / EFA_FIXTURES[operation]).read_bytes()

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request)
        return httpx.Response(200, content=body)

    with httpx.Client(transport=httpx.MockTransport(handler)) as http:
        getattr(Client(http_client=http), operation)(EFA, *args, **options)
    return sent


# The generated variants of `Place` and `TripTime` are not typed as such.
@pytest.mark.parametrize(
    ("operation", "args", "options", "query"),
    [
        (
            "locations",
            ["Hauptbahnhof"],
            {"kinds": [LocationKind.STOP], "limit": 3},
            LocationQuery(text="Hauptbahnhof", kinds=[LocationKind.STOP], limit=3),
        ),
        (
            "departures",
            ["de:14612:28"],
            {"time": TIME, "limit": 7},
            DeparturesQuery(
                stop=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
                time=TIME,
                limit=7,
            ),
        ),
        (
            "departures",
            [FOREIGN_STOP],
            {},
            DeparturesQuery(
                stop=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
            ),
        ),
        (
            "trips",
            [FOREIGN_STOP, "de:14612:13"],
            {"depart_at": TIME, "limit": 3},
            TripsQuery(
                origin=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
                destination=Place.ID("de:14612:13"),  # pyright: ignore[reportArgumentType]
                time=TripTime.DEPART_AT(TIME),  # pyright: ignore[reportArgumentType]
                limit=3,
            ),
        ),
        (
            "trips",
            ["de:14612:28", DRESDEN],
            {"arrive_by": TIME},
            TripsQuery(
                origin=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
                destination=Place.COORDINATE(DRESDEN),  # pyright: ignore[reportArgumentType]
                time=TripTime.ARRIVE_BY(TIME),  # pyright: ignore[reportArgumentType]
            ),
        ),
        (
            "trips",
            ["de:14612:28", "de:14612:13"],
            {"page": LATER},
            TripsQuery(
                origin=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
                destination=Place.ID("de:14612:13"),  # pyright: ignore[reportArgumentType]
                page=LATER,
            ),
        ),
    ],
)
def test_plain_call_sends_the_request_of_the_query_with_the_same_values(
    operation: str, args: list[object], options: dict[str, object], query: object
) -> None:
    request = getattr(EFA, f"{operation}_request")(query)

    sent = sent_by(operation, *args, **options)

    assert [(r.method, r.url) for r in sent] == [
        (request.method.name, httpx.URL(request.url))
    ]


@pytest.mark.parametrize(
    ("operation", "args", "options", "error"),
    [
        ("locations", [LOCATIONS], {"limit": 3}, TypeError),
        ("locations", [LOCATIONS], {"kinds": [LocationKind.STOP]}, TypeError),
        (
            "departures",
            [DeparturesQuery(stop=Place.ID("x"))],  # pyright: ignore[reportArgumentType]
            {"time": TIME},
            TypeError,
        ),
        (
            "trips",
            [
                TripsQuery(
                    origin=Place.ID("x"),  # pyright: ignore[reportArgumentType]
                    destination=Place.ID("y"),  # pyright: ignore[reportArgumentType]
                )
            ],
            {"depart_at": TIME},
            TypeError,
        ),
        ("trips", ["x"], {}, TypeError),
        ("trips", ["x", "y"], {"depart_at": TIME, "arrive_by": TIME}, ValueError),
        # A destination next to a query that has one, also an empty value.
        (
            "trips",
            [
                TripsQuery(
                    origin=Place.ID("x"),  # pyright: ignore[reportArgumentType]
                    destination=Place.ID("y"),  # pyright: ignore[reportArgumentType]
                ),
                (),
            ],
            {},
            TypeError,
        ),
        # Not a stop, with a message that names what is one.
        ("departures", [None], {}, TypeError),
    ],
)
def test_call_that_mixes_or_misses_arguments_is_refused_before_a_request(
    operation: str,
    args: list[object],
    options: dict[str, object],
    error: type[Exception],
) -> None:
    sent: list[httpx.Request] = []
    with pytest.raises(error):
        sent = sent_by(operation, *args, **options)
    assert sent == []


def test_location_the_network_cannot_name_is_an_invalid_query() -> None:
    with pytest.raises(Error.InvalidQuery):
        sent_by("departures", foreign(LocationKind.POI))


def test_async_client_takes_the_plain_form_too() -> None:
    request = EFA.trips_request(
        TripsQuery(
            origin=Place.ID("de:14612:28"),  # pyright: ignore[reportArgumentType]
            destination=Place.COORDINATE(DRESDEN),  # pyright: ignore[reportArgumentType]
            time=TripTime.DEPART_AT(TIME),  # pyright: ignore[reportArgumentType]
        )
    )
    sent: list[httpx.URL] = []

    def handler(request: httpx.Request) -> httpx.Response:
        sent.append(request.url)
        return httpx.Response(
            200, content=(FIXTURES / "efa/vvo-trip.json").read_bytes()
        )

    async def run() -> None:
        async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as http:
            client = AsyncClient(http_client=http)
            await client.trips(EFA, FOREIGN_STOP, DRESDEN, depart_at=TIME)

    asyncio.run(run())

    assert sent == [httpx.URL(request.url)]


def test_place_is_taken_where_a_stop_or_an_end_of_a_trip_is() -> None:
    # `Network.place` returns one, so it has to go back in.
    stop = Place.ID("de:14612:28")
    [by_place] = sent_by("departures", stop)
    [by_id] = sent_by("departures", "de:14612:28")
    assert by_place.url == by_id.url
