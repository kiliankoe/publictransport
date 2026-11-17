"""Client for public transport APIs: EFA, MOTIS, TRIAS and OJP.

`Client` and `AsyncClient` send requests with httpx. The model types and the
functions that build requests and parse responses come from the Rust library
of the same name; `Network.locations_request` and its siblings are there for
applications that bring their own HTTP stack.
"""

from publictransport._client import AsyncClient, Client, ResponseTooLarge
from publictransport._uniffi import (
    Coordinate,
    Departure,
    DeparturesQuery,
    Error,
    Header,
    HttpRequest,
    HttpResponse,
    InternalError,
    Leg,
    LegEnd,
    LegKind,
    Line,
    Location,
    LocationKind,
    LocationQuery,
    Method,
    Mode,
    Network,
    Place,
    Protocol,
    Stopover,
    Trip,
    TripsQuery,
    TripTime,
)

__all__ = [
    "AsyncClient",
    "Client",
    "Coordinate",
    "Departure",
    "DeparturesQuery",
    "Error",
    "Header",
    "HttpRequest",
    "HttpResponse",
    "InternalError",
    "Leg",
    "LegEnd",
    "LegKind",
    "Line",
    "Location",
    "LocationKind",
    "LocationQuery",
    "Method",
    "Mode",
    "Network",
    "Place",
    "Protocol",
    "ResponseTooLarge",
    "Stopover",
    "Trip",
    "TripTime",
    "TripsQuery",
]
