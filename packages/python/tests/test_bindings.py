"""What the hand-written parts of the Rust bindings add: times and helpers."""

import json
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pytest
from publictransport import (
    BundledNetwork,
    Coordinate,
    DeparturesQuery,
    Error,
    GlobalId,
    HafasOptions,
    Header,
    HttpResponse,
    IdScheme,
    LegEnd,
    Location,
    LocationKind,
    LocationQuery,
    Network,
    Place,
    Protocol,
    Registry,
    TripsQuery,
    network_from_json,
)

MOTIS = Network(
    id="t", name="T", endpoint="https://motis.example/api", protocol=Protocol.MOTIS
)


def stop(timezone: str | None = None) -> Location:
    return Location(
        id="a",
        global_ids=[GlobalId(scheme=IdScheme.IFOPT, value="de:14612:28:3:2")],
        kind=LocationKind.STOP,
        name="A",
        locality=None,
        coordinate=None,
        timezone=timezone,
    )


def leg_end(time: datetime) -> LegEnd:
    return LegEnd(
        stop=stop(),
        scheduled_time=time,
        realtime_time=None,
        scheduled_platform=None,
        realtime_platform=None,
        cancelled=False,
    )


@pytest.mark.parametrize(
    ("time", "expected"),
    [
        (datetime(2026, 11, 16, 14, 24, tzinfo=timezone(timedelta(hours=1))), None),
        # Fractions of a second are cut off, also before 1970.
        (
            datetime(2026, 11, 16, 13, 24, 0, 999_999, tzinfo=timezone.utc),
            datetime(2026, 11, 16, 13, 24, tzinfo=timezone.utc),
        ),
        (
            datetime(1960, 1, 1, 0, 0, 0, 1, tzinfo=timezone.utc),
            datetime(1960, 1, 1, tzinfo=timezone.utc),
        ),
    ],
)
def test_time_crosses_the_boundary_as_an_aware_utc_datetime(
    time: datetime, expected: datetime | None
) -> None:
    result = leg_end(time).expected_time()

    assert result == (expected or time)
    assert result.utcoffset() == timedelta(0)


@pytest.mark.parametrize(
    ("zone", "time", "on_the_clock"),
    [
        # Asked with a time of another zone, shown on the day after.
        (
            "Europe/Berlin",
            datetime(2026, 7, 1, 18, 30, tzinfo=timezone(timedelta(hours=-4))),
            "2026-07-02T00:30:00+02:00",
        ),
        # Behind UTC by less than an hour, with seconds.
        (
            "Africa/Monrovia",
            datetime(1960, 1, 1, tzinfo=timezone.utc),
            "1959-12-31T23:15:30-00:44:30",
        ),
    ],
)
def test_local_time_is_the_same_instant_on_the_clocks_of_the_location(
    zone: str, time: datetime, on_the_clock: str
) -> None:
    result = stop(zone).local_time(time)

    assert result is not None
    assert result == time
    assert result.isoformat() == on_the_clock


def test_conveniences_of_departures_and_trips_cross_the_boundary() -> None:
    fixtures = Path(__file__).parents[3] / "crates/core/tests/fixtures/efa"
    vvo = BundledNetwork.VVO_EFA.network()

    def answer(name: str) -> HttpResponse:
        body = (fixtures / name).read_bytes()
        return HttpResponse(status=200, headers=[], body=body)

    departure = vvo.parse_departures(answer("vvo-departures.json"))[0]
    trip = vvo.parse_trips(answer("vvo-trip.json")).trips[0]
    start = trip.legs[0].departure
    scheduled = departure.local_scheduled_time()
    expected = departure.local_expected_time()

    assert scheduled is not None
    assert expected is not None
    assert scheduled.isoformat() == "2026-11-16T14:24:00+01:00"
    assert expected.isoformat() == "2026-11-16T14:25:00+01:00"
    assert (start.delay_seconds(), start.platform()) == (60, "14")
    assert trip.duration_seconds() == 840


def test_local_time_is_none_without_a_time_zone() -> None:
    assert stop().local_time(datetime.now(timezone.utc)) is None


def test_query_time_reaches_the_request() -> None:
    time = datetime(2026, 11, 16, 14, 24, tzinfo=timezone(timedelta(hours=1)))

    # The generated `Place.ID` is not typed as a `Place`.
    query = DeparturesQuery(
        stop=Place.ID("a"),  # pyright: ignore[reportArgumentType]
        time=time,
    )
    request = MOTIS.departures_request(query)

    assert "time=2026-11-16T13%3A24%3A00Z" in request.url


def test_kinds_of_a_location_query_are_optional_and_reach_the_request() -> None:
    every_kind = MOTIS.locations_request(LocationQuery(text="a"))
    stops = MOTIS.locations_request(LocationQuery(text="a", kinds=[LocationKind.STOP]))

    assert "type=" not in every_kind.url
    assert "type=STOP" in stops.url


def test_page_of_a_trips_answer_continues_the_query() -> None:
    body = (
        b'{"direct":[],"itineraries":[],"previousPageCursor":"a","nextPageCursor":"b"}'
    )

    found = MOTIS.parse_trips(HttpResponse(status=200, headers=[], body=body))
    request = MOTIS.trips_request(
        # The generated `Place.ID` is not typed as a `Place`.
        TripsQuery(
            origin=Place.ID("x"),  # pyright: ignore[reportArgumentType]
            destination=Place.ID("y"),  # pyright: ignore[reportArgumentType]
            page=found.later,
        )
    )

    assert (found.trips, found.earlier, found.later) == ([], "a", "b")
    assert "pageCursor=b" in request.url


def test_station_ids_are_shared_by_a_station_and_its_platforms() -> None:
    assert leg_end(datetime.now(timezone.utc)).stop.station_ids() == [
        GlobalId(scheme=IdScheme.IFOPT, value="de:14612:28")
    ]


def test_registry_finds_the_bundled_networks_that_cover_a_position() -> None:
    dresden = Coordinate(latitude=51.05, longitude=13.74)

    covering = [found.network.id for found in Registry.bundled().covering(dresden)]

    assert "vvo-efa" in covering
    assert "vrr-efa" not in covering


def test_bundled_network_gives_the_network_of_the_registry() -> None:
    # The last one: a member is sent to Rust as its position in the list.
    bundled = BundledNetwork.TRANSITOUS

    assert bundled.id() == "transitous"
    assert bundled.network() == Registry.bundled().get("transitous")


def test_network_is_read_from_a_transport_apis_entry() -> None:
    entry = """{
        "name": "Example",
        "type": {"efa": true},
        "options": {"endpoint": "https://efa.example/base"},
        "coverage": {"anyCoverage": {"region": ["DE"]}}
    }"""

    network = network_from_json("example", entry)

    assert (network.id, network.protocol) == ("example", Protocol.EFA)
    with pytest.raises(Error.InvalidNetwork):
        network_from_json("broken", "{}")


def test_hafas_network_builds_a_request_from_its_options() -> None:
    options = HafasOptions(
        client='{"id": "VBB"}',
        auth='{"aid": "key"}',
        ext=None,
        version="1.33",
        language="de",
        products=[],
    )
    network = Network(
        id="vbb",
        name="VBB",
        endpoint="https://hafas.example/bin/mgate.exe",
        protocol=Protocol.HAFAS_MGATE,
        hafas=options,
    )

    request = network.locations_request(LocationQuery(text="a"))

    assert request.url == "https://hafas.example/bin/mgate.exe"
    body = json.loads(request.body or b"")
    assert (body["client"], body["ver"], body["lang"]) == ({"id": "VBB"}, "1.33", "de")
    assert body["svcReqL"][0]["meth"] == "LocMatch"


def test_printing_leaves_keys_out() -> None:
    header = Header(name="X-Api-Key", value="hunter2")
    options = HafasOptions(client="{}", auth='{"aid": "hunter2"}', version="1.33")

    for text in (str(header), repr(header), str(options), repr(options)):
        assert "hunter2" not in text
