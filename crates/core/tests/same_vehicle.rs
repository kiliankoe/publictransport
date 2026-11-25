//! A vehicle that goes on under another line while the travelers stay
//! seated: no transfer, and no leg for the server's marker.

use publictransport::{HttpResponse, Leg, LegKind, Network, Protocol, Trip, TripsQuery};

const EFA: &[u8] = include_bytes!("fixtures/efa/vvo-same-vehicle.json");
const TRIAS: &[u8] = include_bytes!("fixtures/trias/vvo-same-vehicle.xml");
const OJP: &[u8] = include_bytes!("fixtures/ojp/vvo-same-vehicle.xml");
const OTP: &[u8] = include_bytes!("fixtures/otp/entur-same-vehicle.json");
const EFA_TRANSFER: &[u8] = include_bytes!("fixtures/efa/vvo-trip.json");

#[expect(clippy::panic, reason = "a fixture that cannot be read fails the test")]
fn trips(protocol: Protocol, timezone: &str, body: &[u8]) -> Vec<Trip> {
    Network::new("test", "Test", "https://example.org/base", protocol)
        .with_timezone(timezone)
        .with_requestor_ref("OpenService")
        .parse_trips(
            &TripsQuery::new("a", "b"),
            &HttpResponse {
                status: 200,
                headers: Vec::new(),
                body: body.to_vec(),
            },
        )
        .map_or_else(|error| panic!("{protocol:?}: {error}"), |found| found.trips)
}

fn is_ride(leg: &Leg) -> bool {
    matches!(leg.kind, LegKind::Transit { .. })
}

/// The positions of the rides in a trip.
fn rides(trip: &Trip) -> Vec<usize> {
    let legs = trip.legs.iter().enumerate();
    legs.filter(|(_, leg)| is_ride(leg))
        .map(|(index, _)| index)
        .collect()
}

/// In each answer, every trip with two rides stays in the vehicle; the
/// others have a single ride.
#[test]
fn ride_that_continues_in_the_same_vehicle_follows_the_first_directly_and_is_no_transfer() {
    let answers = [
        (Protocol::Efa, "Europe/Berlin", EFA, 1),
        (Protocol::Trias, "Europe/Berlin", TRIAS, 1),
        (Protocol::Ojp1, "Europe/Berlin", OJP, 1),
        (Protocol::OtpTransmodel, "Europe/Oslo", OTP, 2),
    ];
    for (protocol, timezone, body, continuing) in answers {
        let trips = trips(protocol, timezone, body);
        let mut found = 0;
        for trip in &trips {
            let rides = rides(trip);
            assert!(rides.len() <= 2, "{protocol:?}: {trip:?}");
            if rides.len() == 2 {
                found += 1;
                let [first, second] = [rides[0], rides[1]];
                assert_eq!(second, first + 1, "{protocol:?}: a leg between the rides");
                assert!(trip.legs[second].same_vehicle, "{protocol:?}");
                assert_eq!(trip.transfers(), 0, "{protocol:?}");
            }
            // Only the continuing ride is marked: never a walk, never a first ride.
            let marked: Vec<_> = (0..trip.legs.len())
                .filter(|&index| trip.legs[index].same_vehicle)
                .collect();
            let expected: &[usize] = if rides.len() == 2 { &rides[1..] } else { &[] };
            assert_eq!(marked, expected, "{protocol:?}: {trip:?}");
        }
        assert_eq!(
            found, continuing,
            "{protocol:?}: trips staying in the vehicle"
        );
    }
}

/// Two rides without the mark are a change of vehicles, also when they are
/// next to each other and run the same line.
#[test]
fn rides_without_the_mark_are_a_transfer() {
    // S1, a walk, then tram 11.
    let efa = trips(Protocol::Efa, "Europe/Berlin", EFA_TRANSFER).swap_remove(3);
    // Bus 9 and bus 9 again, but the server no longer says to stay seated.
    let unmarked = String::from_utf8(OTP.to_vec())
        .unwrap()
        .replace("\"staySeated\":true", "\"staySeated\":false");
    let otp = trips(Protocol::OtpTransmodel, "Europe/Oslo", unmarked.as_bytes());
    let otp = otp.iter().filter(|trip| rides(trip).len() == 2);

    let mut checked = 0;
    for trip in std::iter::once(&efa).chain(otp) {
        checked += 1;
        assert_eq!(rides(trip).len(), 2, "{trip:?}");
        assert_eq!(trip.transfers(), 1, "{trip:?}");
        assert!(trip.legs.iter().all(|leg| !leg.same_vehicle), "{trip:?}");
    }
    assert_eq!(checked, 3);
}
