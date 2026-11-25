"""Plan trips between two stops on the OTP server of Entur in Norway.

Then ask for the trips after the first ones.
"""

from publictransport import (
    BundledNetwork,
    Client,
    Header,
    LegEnd,
    LegKind,
    LocationKind,
    Trip,
)

# Servers want to know who asks: name your application and a way to reach you.
USER_AGENT = "publictransport-example (+https://github.com/kiliankoe/publictransport)"


def local_time(end: LegEnd) -> str:
    """Give the expected time of day on the clocks at the stop.

    That is the real-time one if there is one. Times are in UTC, and an end
    of a leg also gives them as on the clocks at its stop, where the server
    names the time zone.
    """
    local = end.local_expected_time()
    return local.strftime("%H:%M") if local else "??:??"


def print_trip(trip: Trip) -> None:
    """Print a trip and its legs."""
    departure, arrival = trip.departure(), trip.arrival()
    duration = trip.duration_seconds()
    times = f"{local_time(departure)} to {local_time(arrival)}"
    print(f"  {times}, {int(duration / 60)} min, {trip.transfers()} change(s)")
    for leg in trip.legs:
        if isinstance(leg.kind, LegKind.TRANSIT):
            # The vehicle of the ride before goes on under this line.
            seated = ", stay seated" if leg.same_vehicle else ""
            how = f"{leg.kind.line.name} to {leg.kind.direction or '?'}{seated}"
        elif isinstance(leg.kind, LegKind.WALK):
            how = "walk"
        else:
            how = "other"
        print(
            f"    {local_time(leg.departure)} {leg.departure.stop.name}"
            f" -> {local_time(leg.arrival)} {leg.arrival.stop.name}: {how}",
        )


network = BundledNetwork.ENTUR.network()
# Entur asks every client for its name, as `company-application`.
network.headers.append(Header(name="ET-Client-Name", value="publictransport-example"))

with Client(user_agent=USER_AGENT) as client:
    origin = client.locations(network, "Oslo S", kinds=[LocationKind.STOP])[0]
    destination = client.locations(
        network, "Lillehammer stasjon", kinds=[LocationKind.STOP]
    )[0]

    # Leaving now. `depart_at=` and `arrive_by=` ask for another time.
    # An end can also be a coordinate or the ID of a stop.
    found = client.trips(network, origin, destination, limit=3)
    print(f"Trips from {origin.name} to {destination.name}:")
    for trip in found.trips:
        print_trip(trip)

    # The same arguments again, with the reference to the trips after these.
    if found.later:
        print("Later trips:")
        more = client.trips(network, origin, destination, limit=3, page=found.later)
        for trip in more.trips:
            print_trip(trip)
