"""List the next departures at a stop on the HAFAS server of VBB in Berlin."""

from datetime import datetime

from publictransport import BundledNetwork, Client, Location, LocationKind

# Servers want to know who asks: name your application and a way to reach you.
USER_AGENT = "publictransport-example (+https://github.com/kiliankoe/publictransport)"


def local_time(stop: Location, time: datetime) -> str:
    """Give the time of day on the clocks at the stop.

    Times are in UTC, and a location knows the time zone to show them in,
    where the server names it.
    """
    local = stop.local_time(time)
    return local.strftime("%H:%M") if local else "??:??"


network = BundledNetwork.VBB_HAFAS.network()

with Client(user_agent=USER_AGENT) as client:
    stop = client.locations(network, "S+U Alexanderplatz", kinds=[LocationKind.STOP])[0]
    # From now on. `time=` asks for another time.
    departures = client.departures(network, stop, limit=10)

print(f"Departures at {stop.name}:")
for departure in departures:
    # Only departures with real-time data have a delay.
    delay = ""
    if departure.realtime_time:
        seconds = (departure.realtime_time - departure.scheduled_time).total_seconds()
        delay = f"{int(seconds / 60):+d}"
    print(
        f"  {local_time(departure.stop, departure.scheduled_time)} {delay:>3}"
        f" {departure.line.name:<5} {departure.direction or '?'}"
        f"{' (cancelled)' if departure.cancelled else ''}",
    )
