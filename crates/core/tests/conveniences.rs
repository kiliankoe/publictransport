//! What every application computes from a departure, a trip or a stopover.

use proptest::prelude::*;
use publictransport::{
    Departure, HttpResponse, Leg, LegEnd, Network, Protocol, Stopover, Timestamp, Trip,
};

const DEPARTURES: &[u8] = include_bytes!("fixtures/motis/transitous-stoptimes.json");
const TRIPS: &[u8] = include_bytes!("fixtures/motis/transitous-plan.json");
const JOURNEY: &[u8] = include_bytes!("fixtures/motis/transitous-journey.json");

/// 2026-11-16T13:24:00Z.
const TIME: Timestamp = Timestamp {
    unix_seconds: 1_794_835_440,
};

fn response(body: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

fn network() -> Network {
    Network::new("test", "Test", "https://example.org/base", Protocol::Motis)
}

/// A departure to assign fields to; it cannot be built outside the crate.
fn departure() -> Departure {
    let Ok(mut departures) = network().parse_departures(
        &publictransport::DeparturesQuery::new("stop"),
        &response(DEPARTURES),
    ) else {
        unreachable!("the fixture is a MOTIS response");
    };
    departures.remove(0)
}

/// A trip to assign fields to.
fn trip() -> Trip {
    let Ok(mut trips) = network().parse_trips(
        &publictransport::TripsQuery::new("from", "to"),
        &response(TRIPS),
    ) else {
        unreachable!("the fixture is a MOTIS response");
    };
    trips.trips.remove(0)
}

fn leg() -> Leg {
    trip().legs.remove(0)
}

/// A stopover to assign fields to.
fn stopover() -> Stopover {
    let Ok(mut journey) = network().parse_journey(&response(JOURNEY)) else {
        unreachable!("the fixture is a MOTIS response");
    };
    journey.stops.remove(0)
}

fn after(time: Timestamp, seconds: i64) -> Timestamp {
    Timestamp {
        unix_seconds: time.unix_seconds + seconds,
    }
}

prop_compose! {
    /// A scheduled time in the years 1970 to 2096 and, where there is a
    /// prediction, a predicted one up to a day before or after it.
    fn times()(
        scheduled in 0_i64..4_000_000_000,
        delay in prop::option::of(-86_400_i64..=86_400),
    ) -> (Timestamp, Option<i64>) {
        (Timestamp { unix_seconds: scheduled }, delay)
    }
}

proptest! {
    #[test]
    fn local_times_of_a_departure_are_those_on_the_clocks_at_its_stop(
        (scheduled, delay) in times(),
        timezone in prop::sample::select(vec![
            Some("Europe/Berlin"),
            Some("America/New_York"),
            Some("Asia/Kolkata"),
            None,
        ]),
    ) {
        let mut departure = departure();
        departure.stop.timezone = timezone.map(str::to_owned);
        departure.scheduled_time = scheduled;
        departure.realtime_time = delay.map(|delay| after(scheduled, delay));
        let expected = after(scheduled, delay.unwrap_or(0));

        prop_assert_eq!(
            departure.local_scheduled_time(),
            departure.stop.local_time(scheduled)
        );
        prop_assert_eq!(
            departure.local_expected_time(),
            departure.stop.local_time(expected)
        );
        prop_assert_eq!(departure.local_scheduled_time().is_some(), timezone.is_some());
    }

    #[test]
    fn delay_of_a_departure_is_how_much_later_than_scheduled_it_is_predicted(
        (scheduled, delay) in times(),
    ) {
        let mut departure = departure();
        departure.scheduled_time = scheduled;
        departure.realtime_time = delay.map(|delay| after(scheduled, delay));

        prop_assert_eq!(departure.delay_seconds(), delay);
    }

    #[test]
    fn duration_of_a_trip_is_from_its_expected_departure_to_its_expected_arrival(
        // Every leg leaves before any leg arrives, so no duration is negative.
        ends in prop::collection::vec(
            (
                (0_i64..1_000_000, prop::option::of(-3600_i64..=3600)),
                (2_000_000_i64..3_000_000, prop::option::of(-3600_i64..=3600)),
            ),
            1..4,
        ),
    ) {
        let mut trip = trip();
        trip.legs = ends
            .iter()
            .map(|&((departure, departure_delay), (arrival, arrival_delay))| {
                let mut leg = leg();
                leg.departure.scheduled_time = Timestamp { unix_seconds: departure };
                leg.departure.realtime_time =
                    departure_delay.map(|delay| Timestamp { unix_seconds: departure + delay });
                leg.arrival.scheduled_time = Timestamp { unix_seconds: arrival };
                leg.arrival.realtime_time =
                    arrival_delay.map(|delay| Timestamp { unix_seconds: arrival + delay });
                leg
            })
            .collect();
        let (Some(&((departure, departure_delay), _)), Some(&(_, (arrival, arrival_delay)))) =
            (ends.first(), ends.last())
        else {
            unreachable!("there is at least one leg");
        };

        prop_assert_eq!(
            trip.duration_seconds(),
            arrival + arrival_delay.unwrap_or(0) - departure - departure_delay.unwrap_or(0)
        );
    }

    #[test]
    fn each_side_of_a_stopover_is_a_leg_end_from_the_fields_of_that_side(
        scheduled_arrival in prop::option::of(0_i64..4_000_000_000),
        realtime_arrival in prop::option::of(0_i64..4_000_000_000),
        scheduled_departure in prop::option::of(0_i64..4_000_000_000),
        realtime_departure in prop::option::of(0_i64..4_000_000_000),
        cancelled in any::<bool>(),
    ) {
        let time = |unix_seconds| Timestamp { unix_seconds };
        let mut stopover = stopover();
        stopover.scheduled_arrival = scheduled_arrival.map(time);
        stopover.realtime_arrival = realtime_arrival.map(time);
        stopover.scheduled_departure = scheduled_departure.map(time);
        stopover.realtime_departure = realtime_departure.map(time);
        stopover.scheduled_platform = Some("2".to_owned());
        stopover.realtime_platform = Some("3".to_owned());
        stopover.cancelled = cancelled;

        let fields = |end: Option<LegEnd>| {
            end.map(|end| {
                (
                    end.stop,
                    end.scheduled_time,
                    end.realtime_time,
                    end.scheduled_platform,
                    end.realtime_platform,
                    end.cancelled,
                )
            })
        };
        // A side without a scheduled time is no leg end, predicted or not.
        let side = |scheduled: Option<i64>, realtime: Option<i64>| {
            scheduled.map(|scheduled| {
                (
                    stopover.stop.clone(),
                    time(scheduled),
                    realtime.map(time),
                    Some("2".to_owned()),
                    Some("3".to_owned()),
                    cancelled,
                )
            })
        };
        prop_assert_eq!(
            fields(stopover.arrival()),
            side(scheduled_arrival, realtime_arrival)
        );
        prop_assert_eq!(
            fields(stopover.departure()),
            side(scheduled_departure, realtime_departure)
        );
    }
}

#[test]
fn platform_of_a_stopover_is_the_real_time_one_before_the_scheduled_one() {
    let cases = [
        (Some("2"), Some("3"), Some("3")),
        (Some("2"), None, Some("2")),
        (None, Some("3"), Some("3")),
        (None, None, None),
    ];
    for (scheduled, realtime, shown) in cases {
        let mut stopover = stopover();
        stopover.scheduled_platform = scheduled.map(str::to_owned);
        stopover.realtime_platform = realtime.map(str::to_owned);
        assert_eq!(stopover.platform(), shown, "{scheduled:?}, {realtime:?}");
    }
}

#[test]
fn platform_of_a_departure_is_the_real_time_one_before_the_scheduled_one() {
    let cases = [
        (Some("2"), Some("3"), Some("3")),
        (Some("2"), None, Some("2")),
        (None, Some("3"), Some("3")),
        (None, None, None),
    ];
    for (scheduled, realtime, shown) in cases {
        let mut departure = departure();
        departure.scheduled_platform = scheduled.map(str::to_owned);
        departure.realtime_platform = realtime.map(str::to_owned);
        assert_eq!(departure.platform(), shown, "{scheduled:?}, {realtime:?}");
    }
}

#[test]
fn each_end_of_a_leg_answers_from_its_own_fields() {
    let mut leg = leg();

    leg.departure.stop.timezone = Some("Europe/Berlin".to_owned());
    leg.departure.scheduled_time = TIME;
    leg.departure.realtime_time = Some(after(TIME, 120));
    leg.departure.scheduled_platform = Some("2".to_owned());
    leg.departure.realtime_platform = Some("3".to_owned());

    leg.arrival.stop.timezone = Some("America/New_York".to_owned());
    leg.arrival.scheduled_time = after(TIME, 3600);
    leg.arrival.realtime_time = None;
    leg.arrival.scheduled_platform = Some("5".to_owned());
    leg.arrival.realtime_platform = None;

    let shown = [&leg.departure, &leg.arrival].map(|end| {
        (
            end.local_scheduled_time().map(|local| local.to_string()),
            end.local_expected_time().map(|local| local.to_string()),
            end.delay_seconds(),
            end.platform(),
        )
    });
    let clock = |text: &str| Some(text.to_owned());
    assert_eq!(
        shown,
        [
            (clock("14:24"), clock("14:26"), Some(120), Some("3")),
            (clock("09:24"), clock("09:24"), None, Some("5")),
        ]
    );
}
