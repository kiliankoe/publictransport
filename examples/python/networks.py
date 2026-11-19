"""List the bundled networks that cover a position.

This sends no request: the networks and their areas ship with the package.
"""

from publictransport import Coordinate, Registry

registry = Registry.bundled()
print(f"{len(registry.networks())} networks are bundled.")

duesseldorf = Coordinate(latitude=51.2198, longitude=6.7943)
# The best coverage comes first: real-time data, then complete timetables,
# then any data.
print("At Düsseldorf Hauptbahnhof:")
for covering in registry.covering(duesseldorf):
    network = covering.network
    tier, protocol = covering.tier.name, network.protocol.name
    print(f"  {tier}: {network.name} ({network.id}, {protocol})")

# A network by its ID, as an application would store it.
network = registry.get("vrr-efa")
if network:
    print(f"vrr-efa is {network.name} at {network.endpoint}")
