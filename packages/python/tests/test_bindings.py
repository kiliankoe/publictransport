"""What the hand-written parts of the Rust bindings add: times and helpers."""

from datetime import datetime, timedelta, timezone

import pytest
from publictransport import (
    Coordinate,
    DeparturesQuery,
    Error,
    GlobalId,
    IdScheme,
    LegEnd,
    Location,
    LocationKind,
    LocationQuery,
    Network,
    Protocol,
    Registry,
    network_from_json,
)

MOTIS = Network(
    id="t", name="T", endpoint="https://motis.example/api", protocol=Protocol.MOTIS
)


def leg_end(time: datetime) -> LegEnd:
    stop = Location(
        id="a",
        global_ids=[GlobalId(scheme=IdScheme.IFOPT, value="de:14612:28:3:2")],
        kind=LocationKind.STOP,
        name="A",
        locality=None,
        coordinate=None,
        timezone=None,
    )
    return LegEnd(
        stop=stop,
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


def test_query_time_reaches_the_request() -> None:
    time = datetime(2026, 11, 16, 14, 24, tzinfo=timezone(timedelta(hours=1)))

    request = MOTIS.departures_request(DeparturesQuery(stop_id="a", time=time))

    assert "time=2026-11-16T13%3A24%3A00Z" in request.url


def test_kinds_of_a_location_query_are_optional_and_reach_the_request() -> None:
    every_kind = MOTIS.locations_request(LocationQuery(text="a"))
    stops = MOTIS.locations_request(LocationQuery(text="a", kinds=[LocationKind.STOP]))

    assert "type=" not in every_kind.url
    assert "type=STOP" in stops.url


def test_station_ids_are_shared_by_a_station_and_its_platforms() -> None:
    assert leg_end(datetime.now(timezone.utc)).stop.station_ids() == [
        GlobalId(scheme=IdScheme.IFOPT, value="de:14612:28")
    ]


def test_registry_finds_the_bundled_networks_that_cover_a_position() -> None:
    dresden = Coordinate(latitude=51.05, longitude=13.74)

    covering = [found.network.id for found in Registry.bundled().covering(dresden)]

    assert "vvo-efa" in covering
    assert "vrr-efa" not in covering


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
