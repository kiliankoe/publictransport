"""Search for places by name on the EFA server of VVO in Dresden."""

from publictransport import BundledNetwork, Client

# Servers want to know who asks: name your application and a way to reach you.
USER_AGENT = "publictransport-example (+https://github.com/kiliankoe/publictransport)"

network = BundledNetwork.VVO_EFA.network()

with Client(user_agent=USER_AGENT) as client:
    # Every kind of place. `kinds=[LocationKind.STOP]` asks for stops only.
    locations = client.locations(network, "Dresden Zwinger")

# The best match comes first.
for location in locations[:5]:
    position = ""
    if location.coordinate:
        latitude, longitude = (
            location.coordinate.latitude,
            location.coordinate.longitude,
        )
        position = f" at {latitude:.4f}, {longitude:.4f}"
    print(f"{location.kind.name}: {location.name} ({location.id}){position}")
