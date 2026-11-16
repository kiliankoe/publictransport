"""What the hand-written parts of the Rust bindings add: times and helpers."""

from datetime import datetime, timedelta, timezone

import pytest
from publictransport import (
    DeparturesQuery,
    LegEnd,
    Location,
    LocationKind,
    Network,
    Protocol,
)

MOTIS = Network(
    id="t", name="T", endpoint="https://motis.example/api", protocol=Protocol.MOTIS
)


def leg_end(time: datetime) -> LegEnd:
    stop = Location(
        id="a",
        global_id="de:14612:28:3:2",
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


def test_station_id_is_shared_by_a_station_and_its_platforms() -> None:
    assert leg_end(datetime.now(timezone.utc)).stop.station_id() == "de:14612:28"
