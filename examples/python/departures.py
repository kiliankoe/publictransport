"""List the next departures at a stop on the HAFAS server of VBB in Berlin."""

from publictransport import BundledNetwork, Client, LocationKind, RemarkKind

# Servers want to know who asks: name your application and a way to reach you.
USER_AGENT = "publictransport-example (+https://github.com/kiliankoe/publictransport)"

network = BundledNetwork.VBB_HAFAS.network()

with Client(user_agent=USER_AGENT) as client:
    stop = client.locations(network, "S+U Alexanderplatz", kinds=[LocationKind.STOP])[0]
    # From now on. `time=` asks for another time.
    departures = client.departures(network, stop, limit=10)
    # The run of the first vehicle: every stop it calls at. The ID only works
    # on the network it came from, and for about a day.
    journey_id = next((each.journey_id for each in departures if each.journey_id), None)
    journey = client.journey(network, journey_id) if journey_id else None

print(f"Departures at {stop.name}:")
for departure in departures:
    # Times are in UTC. A departure also gives them as on the clocks at its
    # stop, where the server names the time zone.
    local = departure.local_scheduled_time()
    time = f"{local:%H:%M}" if local else "??:??"
    # Only departures with real-time data have a delay.
    seconds = departure.delay_seconds()
    delay = "" if seconds is None else f"{int(seconds / 60):+d}"
    print(
        f"  {time} {delay:>3}"
        f" {departure.line.name:<5} {departure.direction or '?'}"
        f"{' (cancelled)' if departure.cancelled else ''}",
    )
    # What the operator reports about the ride, such as a closed road.
    for remark in departure.remarks:
        if remark.kind == RemarkKind.DISRUPTION and remark.title:
            print(f"            ! {remark.title}")

if journey:
    print(f"Stops of {journey.line.name}:")
    for stopover in journey.stops:
        # Each side of a stopover is an end of a leg, with the helpers of
        # a departure. The last stop has no departure.
        end = stopover.departure() or stopover.arrival()
        local = end.local_expected_time() if end else None
        print(f"  {local:%H:%M}" if local else "  ??:??", stopover.stop.name)
